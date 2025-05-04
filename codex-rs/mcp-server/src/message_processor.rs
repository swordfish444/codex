//! Very small proof-of-concept request router for the MCP prototype server.

use mcp_types::CallToolRequestParams;
use mcp_types::CallToolResult;
use mcp_types::CallToolResultContent;
use mcp_types::ClientRequest;
use mcp_types::JSONRPCBatchRequest;
use mcp_types::JSONRPCBatchResponse;
use mcp_types::JSONRPCError;
use mcp_types::JSONRPCErrorError;
use mcp_types::JSONRPCMessage;
use mcp_types::JSONRPCNotification;
use mcp_types::JSONRPCRequest;
use mcp_types::JSONRPCResponse;
use mcp_types::ListToolsResult;
use mcp_types::ModelContextProtocolRequest;
use mcp_types::RequestId;
use mcp_types::ServerCapabilitiesTools;
use mcp_types::ServerNotification;
use mcp_types::TextContent;
use mcp_types::Tool;
use mcp_types::ToolInputSchema;
use mcp_types::JSONRPC_VERSION;
use serde_json::json;
use tokio::task;

// Import types from codex-core.
use codex_core::codex_wrapper::init_codex;
use codex_core::config::Config as CodexConfig;

// Config object accepted by the `codex` tool-call.
use crate::codex_tool_config::ConfigForToolCall as CodexToolConfig;
use codex_core::protocol::{Event, EventMsg};

// Helper to convert a Codex Event into an MCP JSON-RPC notification.
fn codex_event_to_notification(event: &Event) -> JSONRPCMessage {
    JSONRPCMessage::Notification(JSONRPCNotification {
        jsonrpc: JSONRPC_VERSION.into(),
        method: "codex/event".into(),
        params: Some(serde_json::to_value(event).expect("Event must serialize")),
    })
}
use tokio::sync::mpsc;

pub(crate) struct MessageProcessor {
    outgoing: mpsc::Sender<JSONRPCMessage>,
    initialized: bool,
}

impl MessageProcessor {
    /// Create a new `MessageProcessor`, retaining a handle to the outgoing
    /// `Sender` so handlers can enqueue messages to be written to stdout.
    pub(crate) fn new(outgoing: mpsc::Sender<JSONRPCMessage>) -> Self {
        Self {
            outgoing,
            initialized: false,
        }
    }

    pub(crate) fn process_request(&mut self, request: JSONRPCRequest) {
        // Hold on to the ID so we can respond.
        let request_id = request.id.clone();

        let client_request = match ClientRequest::try_from(request) {
            Ok(client_request) => client_request,
            Err(e) => {
                tracing::warn!("Failed to convert request: {e}");
                return;
            }
        };

        // Dispatch to a dedicated handler for each request type.
        match client_request {
            ClientRequest::InitializeRequest(params) => {
                self.handle_initialize(request_id, params);
            }
            ClientRequest::PingRequest(params) => {
                self.handle_ping(request_id, params);
            }
            ClientRequest::ListResourcesRequest(params) => {
                self.handle_list_resources(params);
            }
            ClientRequest::ListResourceTemplatesRequest(params) => {
                self.handle_list_resource_templates(params);
            }
            ClientRequest::ReadResourceRequest(params) => {
                self.handle_read_resource(params);
            }
            ClientRequest::SubscribeRequest(params) => {
                self.handle_subscribe(params);
            }
            ClientRequest::UnsubscribeRequest(params) => {
                self.handle_unsubscribe(params);
            }
            ClientRequest::ListPromptsRequest(params) => {
                self.handle_list_prompts(params);
            }
            ClientRequest::GetPromptRequest(params) => {
                self.handle_get_prompt(params);
            }
            ClientRequest::ListToolsRequest(params) => {
                self.handle_list_tools(request_id, params);
            }
            ClientRequest::CallToolRequest(params) => {
                self.handle_call_tool(request_id, params);
            }
            ClientRequest::SetLevelRequest(params) => {
                self.handle_set_level(params);
            }
            ClientRequest::CompleteRequest(params) => {
                self.handle_complete(params);
            }
        }
    }

    /// Handle a standalone JSON-RPC response originating from the peer.
    pub(crate) fn process_response(&mut self, response: JSONRPCResponse) {
        tracing::info!("<- response: {:?}", response);
    }

    /// Handle a fire-and-forget JSON-RPC notification.
    pub(crate) fn process_notification(&mut self, notification: JSONRPCNotification) {
        let server_notification = match ServerNotification::try_from(notification) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("Failed to convert notification: {e}");
                return;
            }
        };

        // Similar to requests, route each notification type to its own stub
        // handler so additional logic can be implemented incrementally.
        match server_notification {
            ServerNotification::CancelledNotification(params) => {
                self.handle_cancelled_notification(params);
            }
            ServerNotification::ProgressNotification(params) => {
                self.handle_progress_notification(params);
            }
            ServerNotification::ResourceListChangedNotification(params) => {
                self.handle_resource_list_changed(params);
            }
            ServerNotification::ResourceUpdatedNotification(params) => {
                self.handle_resource_updated(params);
            }
            ServerNotification::PromptListChangedNotification(params) => {
                self.handle_prompt_list_changed(params);
            }
            ServerNotification::ToolListChangedNotification(params) => {
                self.handle_tool_list_changed(params);
            }
            ServerNotification::LoggingMessageNotification(params) => {
                self.handle_logging_message(params);
            }
        }
    }

    /// Handle a batch of requests and/or notifications.
    pub(crate) fn process_batch_request(&mut self, batch: JSONRPCBatchRequest) {
        tracing::info!("<- batch request containing {} item(s)", batch.len());
        for item in batch {
            match item {
                mcp_types::JSONRPCBatchRequestItem::JSONRPCRequest(req) => {
                    self.process_request(req);
                }
                mcp_types::JSONRPCBatchRequestItem::JSONRPCNotification(note) => {
                    self.process_notification(note);
                }
            }
        }
    }

    /// Handle an error object received from the peer.
    pub(crate) fn process_error(&mut self, err: JSONRPCError) {
        tracing::error!("<- error: {:?}", err);
    }

    /// Handle a batch of responses/errors.
    pub(crate) fn process_batch_response(&mut self, batch: JSONRPCBatchResponse) {
        tracing::info!("<- batch response containing {} item(s)", batch.len());
        for item in batch {
            match item {
                mcp_types::JSONRPCBatchResponseItem::JSONRPCResponse(resp) => {
                    self.process_response(resp);
                }
                mcp_types::JSONRPCBatchResponseItem::JSONRPCError(err) => {
                    self.process_error(err);
                }
            }
        }
    }

    fn handle_initialize(
        &mut self,
        id: RequestId,
        params: <mcp_types::InitializeRequest as ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("initialize -> params: {:?}", params);

        if self.initialized {
            // Already initialised: send JSON-RPC error response.
            let error_msg = JSONRPCMessage::Error(JSONRPCError {
                jsonrpc: JSONRPC_VERSION.into(),
                id,
                error: JSONRPCErrorError {
                    code: -32600, // Invalid Request
                    message: "initialize called more than once".to_string(),
                    data: None,
                },
            });

            if let Err(e) = self.outgoing.try_send(error_msg) {
                tracing::error!("Failed to send initialization error: {e}");
            }
            return;
        }

        self.initialized = true;

        // Build a minimal InitializeResult. Fill with placeholders.
        let result = mcp_types::InitializeResult {
            capabilities: mcp_types::ServerCapabilities {
                completions: None,
                experimental: None,
                logging: None,
                prompts: None,
                resources: None,
                tools: Some(ServerCapabilitiesTools {
                    list_changed: Some(true),
                }),
            },
            instructions: None,
            protocol_version: params.protocol_version.clone(),
            server_info: mcp_types::Implementation {
                name: "codex-mcp-server".to_string(),
                version: mcp_types::MCP_SCHEMA_VERSION.to_string(),
            },
        };

        self.send_response::<mcp_types::InitializeRequest>(id, result);
    }

    fn send_response<T>(&self, id: RequestId, result: T::Result)
    where
        T: ModelContextProtocolRequest,
    {
        let response = JSONRPCMessage::Response(JSONRPCResponse {
            jsonrpc: JSONRPC_VERSION.into(),
            id,
            result: serde_json::to_value(result).unwrap(),
        });

        if let Err(e) = self.outgoing.try_send(response) {
            tracing::error!("Failed to send response: {e}");
        }
    }

    fn handle_ping(
        &self,
        id: RequestId,
        params: <mcp_types::PingRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("ping -> params: {:?}", params);
        let result = json!({});
        self.send_response::<mcp_types::PingRequest>(id, result);
    }

    fn handle_list_resources(
        &self,
        params: <mcp_types::ListResourcesRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/list -> params: {:?}", params);
    }

    fn handle_list_resource_templates(
        &self,
        params:
            <mcp_types::ListResourceTemplatesRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/templates/list -> params: {:?}", params);
    }

    fn handle_read_resource(
        &self,
        params: <mcp_types::ReadResourceRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/read -> params: {:?}", params);
    }

    fn handle_subscribe(
        &self,
        params: <mcp_types::SubscribeRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/subscribe -> params: {:?}", params);
    }

    fn handle_unsubscribe(
        &self,
        params: <mcp_types::UnsubscribeRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/unsubscribe -> params: {:?}", params);
    }

    fn handle_list_prompts(
        &self,
        params: <mcp_types::ListPromptsRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("prompts/list -> params: {:?}", params);
    }

    fn handle_get_prompt(
        &self,
        params: <mcp_types::GetPromptRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("prompts/get -> params: {:?}", params);
    }

    fn handle_list_tools(
        &self,
        id: RequestId,
        params: <mcp_types::ListToolsRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::trace!("tools/list -> {params:?}");
        // -----------------------------------------------------------------
        // Build a *flattened* JSON Schema for the Codex tool’s config.  Using
        // the full `schemars` output would introduce `$ref`s which MCP tool
        // schemas do not support (they allow only `type`, `properties` and
        // `required`).  Therefore we manually construct a minimal-but-useful
        // schema containing just primitive types and string enums.
        // -----------------------------------------------------------------

        let properties = codex_tool_properties();

        // Required fields mirror the non-optional struct members.
        let required = codex_tool_required();

        let result = ListToolsResult {
            tools: vec![Tool {
                name: "codex".to_string(),
                input_schema: ToolInputSchema {
                    r#type: "object".to_string(),
                    properties: Some(properties),
                    required: Some(required),
                },
                description: Some(
                    "Run a Codex session. Accepts configuration parameters matching the Codex Config struct.".to_string(),
                ),
                annotations: None,
            }],
            next_cursor: None,
        };

        self.send_response::<mcp_types::ListToolsRequest>(id, result);
    }

    fn handle_call_tool(
        &self,
        id: RequestId,
        params: <mcp_types::CallToolRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("tools/call -> params: {:?}", params);
        let CallToolRequestParams { name, arguments } = params;

        // We only support the "codex" tool for now.
        if name != "codex" {
            // Tool not found – return error result so the LLM can react.
            let result = CallToolResult {
                content: vec![CallToolResultContent::TextContent(TextContent {
                    r#type: "text".to_string(),
                    text: format!("Unknown tool '{name}'"),
                    annotations: None,
                })],
                is_error: Some(true),
            };
            self.send_response::<mcp_types::CallToolRequest>(id, result);
            return;
        }

        // -----------------------------------------------------------------
        // Parse arguments synchronously so that we can fail fast **before**
        // spawning the async session task.  This keeps the control-flow easy
        // to reason about and avoids spawning a task that immediately errors
        // out.
        // -----------------------------------------------------------------

        let config: CodexConfig = match arguments {
            Some(json_val) => {
                match serde_json::from_value::<CodexToolConfig>(json_val) {
                    Ok(tool_cfg) => match tool_cfg.into_config() {
                        Ok(cfg) => cfg,
                        Err(e) => {
                            let result = CallToolResult {
                                content: vec![CallToolResultContent::TextContent(TextContent {
                                    r#type: "text".to_owned(),
                                    text: format!(
                                        "Failed to load Codex configuration from overrides: {e}"
                                    ),
                                    annotations: None,
                                })],
                                is_error: Some(true),
                            };
                            self.send_response::<mcp_types::CallToolRequest>(id, result);
                            return;
                        }
                    },
                    Err(e) => {
                        let result = CallToolResult {
                            content: vec![CallToolResultContent::TextContent(TextContent {
                                r#type: "text".to_owned(),
                                text: format!("Failed to parse configuration for Codex tool: {e}"),
                                annotations: None,
                            })],
                            is_error: Some(true),
                        };
                        self.send_response::<mcp_types::CallToolRequest>(id, result);
                        return;
                    }
                }
            }
            None => {
                let result = CallToolResult {
                    content: vec![CallToolResultContent::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text: "Missing arguments for codex tool-call; the `prompt` field is required.".to_string(),
                        annotations: None,
                    })],
                    is_error: Some(true),
                };
                self.send_response::<mcp_types::CallToolRequest>(id, result);
                return;
            }
        };

        // Clone outgoing sender to move into async task.
        let outgoing = self.outgoing.clone();

        // Spawn an async task to handle the Codex session so that we do not
        // block the synchronous message-processing loop.
        task::spawn(async move {

            // -----------------------------------------------------------------
            // Step 1:  Start Codex session (config already prepared).
            // -----------------------------------------------------------------
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
                            id: id.clone(),
                            result: result.into(),
                        }))
                        .await;
                    return;
                }
            };

            // Send the initial SessionConfigured event as a notification so the
            // client can begin rendering.
            let _ = outgoing.send(codex_event_to_notification(&first_event)).await;

            // We'll track the last AgentMessage so we can fulfil the tool call
            // response when the task completes.
            let mut last_agent_message: Option<String> = None;

            // -----------------------------------------------------------------
            // Step 3: Pump events until we reach a state that requires a tool
            // response.
            // -----------------------------------------------------------------
            loop {
                match codex.next_event().await {
                    Ok(event) => {
                        // Forward all events to the MCP client.
                        let _ = outgoing.send(codex_event_to_notification(&event)).await;

                        match &event.msg {
                            EventMsg::AgentMessage { message } => {
                                last_agent_message = Some(message.clone());
                            }
                            EventMsg::ExecApprovalRequest { .. } => {
                                // Respond to the original call with an exec approval request.
                                let result = CallToolResult {
                                    content: vec![CallToolResultContent::TextContent(TextContent {
                                        r#type: "text".to_string(),
                                        text: "EXEC_APPROVAL_REQUIRED".to_string(),
                                        annotations: None,
                                    })],
                                    is_error: None,
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
                            EventMsg::ApplyPatchApprovalRequest { .. } => {
                                // Respond to the original call with a patch approval request.
                                let result = CallToolResult {
                                    content: vec![CallToolResultContent::TextContent(TextContent {
                                        r#type: "text".to_string(),
                                        text: "PATCH_APPROVAL_REQUIRED".to_string(),
                                        annotations: None,
                                    })],
                                    is_error: None,
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
                            EventMsg::TaskComplete => {
                                // Return the last agent message, if any.
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
                                            text: "<no-output>".to_string(),
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
                            _ => {
                                // Nothing to do; continue pumping.
                            }
                        }
                    }
                    Err(e) => {
                        // Bubble up error to the user via the response.
                        let result = CallToolResult {
                            content: vec![CallToolResultContent::TextContent(TextContent {
                                r#type: "text".to_string(),
                                text: format!("Codex session error: {e}"),
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
        });
    }

    fn handle_set_level(
        &self,
        params: <mcp_types::SetLevelRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("logging/setLevel -> params: {:?}", params);
    }

    fn handle_complete(
        &self,
        params: <mcp_types::CompleteRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("completion/complete -> params: {:?}", params);
    }

    // ---------------------------------------------------------------------
    // Notification handlers
    // ---------------------------------------------------------------------

    fn handle_cancelled_notification(
        &self,
        params: <mcp_types::CancelledNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/cancelled -> params: {:?}", params);
    }

    fn handle_progress_notification(
        &self,
        params: <mcp_types::ProgressNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/progress -> params: {:?}", params);
    }

    fn handle_resource_list_changed(
        &self,
        params: <mcp_types::ResourceListChangedNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!(
            "notifications/resources/list_changed -> params: {:?}",
            params
        );
    }

    fn handle_resource_updated(
        &self,
        params: <mcp_types::ResourceUpdatedNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/resources/updated -> params: {:?}", params);
    }

    fn handle_prompt_list_changed(
        &self,
        params: <mcp_types::PromptListChangedNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/prompts/list_changed -> params: {:?}", params);
    }

    fn handle_tool_list_changed(
        &self,
        params: <mcp_types::ToolListChangedNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/tools/list_changed -> params: {:?}", params);
    }

    fn handle_logging_message(
        &self,
        params: <mcp_types::LoggingMessageNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/message -> params: {:?}", params);
    }
}

// ---------------------------------------------------------------------------
// Helper functions used by both production code and tests.
// ---------------------------------------------------------------------------

/// JSON Schema `properties` object for the Codex tool.
fn codex_tool_properties() -> serde_json::Value {
    json!({
        "prompt": { "type": "string", "description": "Initial user prompt", "minLength": 1 },
        "model": { "type": "string", "description": "Model name to use" },
        "approval-policy": {
            "type": "string",
            "enum": [
                "unless-allow-listed",
                "auto-edit",
                "on-failure",
                "never",
            ],
            "description": "When to request user approval for shell commands"
        },
        "sandbox-permissions": {
            "type": ["array", "null"],
            "items": { "type": "string" },
            "description": "Execution sandbox permissions"
        },
        "disable-response-storage": {
            "type": "boolean",
            "description": "Disable server-side response caching"
        },
        "instructions": { "type": ["string", "null"] },
        "notify": {
            "type": ["array", "null"],
            "items": { "type": "string" }
        },
        "cwd": { "type": "string" }
    })
}

/// Non-optional fields of the Codex tool’s input object.
fn codex_tool_required() -> Vec<String> {
    // All fields are optional so we don’t require anything here.
    vec!["prompt".to_string()] // prompt is now mandatory
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{codex_tool_properties, codex_tool_required};

    #[test]
    fn codex_tool_schema_contains_expected_fields() {
        let props = codex_tool_properties();

        for key in [
            "prompt",
            "model",
            "approval-policy",
            "sandbox-permissions",
            "disable-response-storage",
            "cwd",
        ] {
            assert!(props.get(key).is_some(), "missing property `{key}`");
        }

        // Approval policy enum variants.
        let approval_policy = props.get("approval-policy").unwrap();
        let enum_vals = approval_policy.get("enum").unwrap().as_array().unwrap();
        for expected in [
            "unless-allow-listed",
            "auto-edit",
            "on-failure",
            "never",
        ] {
            assert!(enum_vals.iter().any(|v| v == expected), "enum missing {expected}");
        }

        // All required fields listed are present in properties.
        let required = codex_tool_required();
        for field in required {
            assert!(props.get(&field).is_some(), "required field `{field}` absent from properties");
        }
    }
}
