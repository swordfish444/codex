//! Asynchronous worker that executes a **Codex** tool-call inside a spawned
//! Tokio task. Separated from `message_processor.rs` to keep that file small
//! and to make future feature-growth easier to manage.

use codex_core::codex_wrapper::init_codex;
use codex_core::config::Config as CodexConfig;
use codex_core::protocol::AgentMessageEvent;
use codex_core::protocol::ApplyPatchApprovalRequestEvent;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::ExecApprovalRequestEvent;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use codex_core::protocol::ReviewDecision;
use codex_core::protocol::Submission;
use codex_core::protocol::TaskCompleteEvent;
use mcp_types::CallToolResult;
use mcp_types::CallToolResultContent;
use mcp_types::JSONRPC_VERSION;
use mcp_types::JSONRPCMessage;
use mcp_types::JSONRPCNotification as McpNotification;
use mcp_types::JSONRPCResponse;
use mcp_types::RequestId;
use mcp_types::TextContent;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

/// Convert a Codex [`Event`] to an MCP notification.
///
/// NOTE: This helper is kept local because we only ever emit notifications
/// from within this worker. The implementation is intentionally infallible –
/// serialization failures are treated as bugs.
fn codex_event_to_notification(event: &Event) -> JSONRPCMessage {
    #[expect(clippy::expect_used)]
    JSONRPCMessage::Notification(mcp_types::JSONRPCNotification {
        jsonrpc: JSONRPC_VERSION.into(),
        method: "codex/event".into(),
        params: Some(serde_json::to_value(event).expect("Event must serialize")),
    })
}

/// Run a complete Codex session and stream events back to the client.
///
/// On completion (success or error) the function sends the appropriate
/// `tools/call` response so the LLM can continue the conversation.
pub async fn run_codex_tool_session(
    id: RequestId,
    initial_prompt: String,
    config: CodexConfig,
    outgoing: Sender<JSONRPCMessage>,
    mut approval_rx: Receiver<ReviewDecision>,
) {
    let (codex, first_event, _ctrl_c) = match init_codex(config).await {
        Ok(res) => res,
        Err(e) => {
            let result = CallToolResult {
                content: vec![CallToolResultContent::TextContent(TextContent {
                    r#type: "text".to_string(),
                    text: format!("Failed to start Codex session: {e}"),
                    annotations: None,
                })],
                is_error: Some(true),
            };
            let _ = outgoing
                .send(JSONRPCMessage::Response(JSONRPCResponse {
                    jsonrpc: JSONRPC_VERSION.into(),
                    id,
                    result: result.into(),
                }))
                .await;
            return;
        }
    };

    // Send initial SessionConfigured event.
    let _ = outgoing
        .send(codex_event_to_notification(&first_event))
        .await;

    // Use the original MCP request ID as the `sub_id` for the Codex submission so that
    // any events emitted for this tool-call can be correlated with the
    // originating `tools/call` request.
    let sub_id = match &id {
        RequestId::String(s) => s.clone(),
        RequestId::Integer(n) => n.to_string(),
    };

    let submission = Submission {
        id: sub_id.clone(),
        op: Op::UserInput {
            items: vec![InputItem::Text {
                text: initial_prompt.clone(),
            }],
        },
    };

    if let Err(e) = codex.submit_with_id(submission).await {
        tracing::error!("Failed to submit initial prompt: {e}");
    }

    let mut last_agent_message: Option<String> = None;

    // Stream events until the Codex task completes. When Codex asks for
    // approval we pause, wait for a decision from the MCP client (delivered
    // over `approval_rx` via `codex/approval`), forward the decision, and
    // continue the session.
    loop {
        match codex.next_event().await {
            Ok(event) => {
                let _ = outgoing.send(codex_event_to_notification(&event)).await;

                match &event.msg {
                    EventMsg::AgentMessage(AgentMessageEvent { message }) => {
                        last_agent_message = Some(message.clone());
                    }
                    EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                        command,
                        cwd,
                        reason,
                    }) => {
                        // Dispatch an informational notification so the client can surface a UI.
                        // We intentionally send a *notification* rather than a *request* because most
                        // generic MCP clients (including the Inspector) do not implement a handler for
                        // custom server->client requests and will otherwise respond with -32601.
                        let params = serde_json::json!({
                            "id": sub_id,
                            "kind": "exec",
                            "command": command,
                            "cwd": cwd,
                            "reason": reason,
                        });
                        let _ = outgoing
                            .send(JSONRPCMessage::Notification(McpNotification {
                                jsonrpc: JSONRPC_VERSION.into(),
                                method: "codex/approval".into(),
                                params: Some(params),
                            }))
                            .await;

                        // Wait for the MCP client to respond with an approval decision.
                        let decision = approval_rx.recv().await.unwrap_or_default();
                        // Forward to Codex.
                        if let Err(e) = codex
                            .submit(Op::ExecApproval {
                                id: event.id.clone(),
                                decision,
                            })
                            .await
                        {
                            tracing::error!("failed to submit ExecApproval op: {e}");
                            break;
                        }
                    }
                    EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                        reason,
                        grant_root,
                        ..
                    }) => {
                        let params = serde_json::json!({
                            "id": sub_id,
                            "kind": "patch",
                            "reason": reason,
                            "grant_root": grant_root,
                        });
                        let _ = outgoing
                            .send(JSONRPCMessage::Notification(McpNotification {
                                jsonrpc: JSONRPC_VERSION.into(),
                                method: "codex/approval".into(),
                                params: Some(params),
                            }))
                            .await;

                        let decision = approval_rx.recv().await.unwrap_or_default();
                        if let Err(e) = codex
                            .submit(Op::PatchApproval {
                                id: event.id.clone(),
                                decision,
                            })
                            .await
                        {
                            tracing::error!("failed to submit PatchApproval op: {e}");
                            break;
                        }
                    }
                    EventMsg::TaskComplete(TaskCompleteEvent { .. }) => {
                        // Session finished – send the final MCP response.
                        let result = if let Some(msg) = last_agent_message {
                            CallToolResult {
                                content: vec![CallToolResultContent::TextContent(TextContent {
                                    r#type: "text".to_string(),
                                    text: msg,
                                    annotations: None,
                                })],
                                is_error: None,
                            }
                        } else {
                            CallToolResult {
                                content: vec![CallToolResultContent::TextContent(TextContent {
                                    r#type: "text".to_string(),
                                    text: String::new(),
                                    annotations: None,
                                })],
                                is_error: None,
                            }
                        };
                        let _ = outgoing
                            .send(JSONRPCMessage::Response(JSONRPCResponse {
                                jsonrpc: JSONRPC_VERSION.into(),
                                id: id.clone(),
                                result: result.into(),
                            }))
                            .await;
                        break;
                    }
                    EventMsg::SessionConfigured(_) => {
                        // Already surfaced above; ignore duplicates.
                    }
                    EventMsg::AgentMessageDelta(_)
                    | EventMsg::AgentReasoningDelta(_)
                    | EventMsg::Error(_)
                    | EventMsg::TaskStarted
                    | EventMsg::TokenCount(_)
                    | EventMsg::AgentReasoning(_)
                    | EventMsg::McpToolCallBegin(_)
                    | EventMsg::McpToolCallEnd(_)
                    | EventMsg::ExecCommandBegin(_)
                    | EventMsg::ExecCommandEnd(_)
                    | EventMsg::BackgroundEvent(_)
                    | EventMsg::PatchApplyBegin(_)
                    | EventMsg::PatchApplyEnd(_)
                    | EventMsg::GetHistoryEntryResponse(_) => {
                        // No special handling.
                    }
                }
            }
            Err(e) => {
                let result = CallToolResult {
                    content: vec![CallToolResultContent::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text: format!("Codex runtime error: {e}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                };
                let _ = outgoing
                    .send(JSONRPCMessage::Response(JSONRPCResponse {
                        jsonrpc: JSONRPC_VERSION.into(),
                        id: id.clone(),
                        result: result.into(),
                    }))
                    .await;
                break;
            }
        }
    }
}
