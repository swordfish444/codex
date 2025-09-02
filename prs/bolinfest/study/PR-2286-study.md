**DOs**
- Boldly handle approval events: add bespoke handling for patch/exec approval events and forward them to the client for a decision.
- Forward as typed server requests: use dedicated param/response structs and stable method strings.
- Include conversation_id: always attach the current `ConversationId` in server-initiated requests.
- Use wire-format constants: reference `APPLY_PATCH_APPROVAL_METHOD` and `EXEC_COMMAND_APPROVAL_METHOD`, never hardcode strings.
- Spawn response tasks: await approval responses in a `tokio::spawn` task to keep the main loop responsive.
- Default to Denied on errors: if deserialization or request flow fails, fall back to `ReviewDecision::Denied` (conservative).
- Submit decisions to the conversation: translate client responses to `Op::PatchApproval` / `Op::ExecApproval`.
- Clone events before serializing: if an event is reused after JSON serialization, clone it first.
- Keep notifications for all events (for now): continue emitting generic `codex/event/{...}` notifications while migrating to a typed, stable format.
- Rename to ClientRequest: reflect directionality now that requests flow both ways; alias MCP’s client request to avoid name clashes.
- Prefer PathBuf and explicit types: e.g., `HashMap<PathBuf, FileChange>` and `Vec<String>` for commands.
- Plan timeouts: add timeouts on approval waits to avoid orphaned tasks.
- Inline format! variables: embed variables directly in `{}`.

```rust
// DO: Clone before serializing and use inline format! variables.
let method = format!("codex/event/{}", event.msg);
let mut params = match serde_json::to_value(event.clone()) {
    Ok(serde_json::Value::Object(map)) => map,
    _ => {
        tracing::error!("event did not serialize to an object");
        return;
    }
};
outgoing
    .send_notification(OutgoingNotification { method, params: Some(params.into()) })
    .await;
```

```rust
// DO: Bespoke handling for approval events, using typed params and constants.
async fn apply_bespoke_event_handling(
    event: Event,
    conversation_id: ConversationId,
    conversation: Arc<CodexConversation>,
    outgoing: Arc<OutgoingMessageSender>,
) {
    let Event { id: event_id, msg } = event;
    match msg {
        EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id: _, changes, reason, grant_root
        }) => {
            let params = ApplyPatchApprovalParams {
                conversation_id,
                file_changes: changes,
                reason,
                grant_root,
            };
            let rx = outgoing
                .send_request(APPLY_PATCH_APPROVAL_METHOD, Some(serde_json::to_value(&params).unwrap_or_default()))
                .await;
            tokio::spawn(async move {
                on_patch_approval_response(event_id, rx, conversation).await;
            });
        }
        EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
            call_id: _, command, cwd, reason
        }) => {
            let params = ExecCommandApprovalParams {
                conversation_id,
                command,
                cwd,
                reason,
            };
            let rx = outgoing
                .send_request(EXEC_COMMAND_APPROVAL_METHOD, Some(serde_json::to_value(&params).unwrap_or_default()))
                .await;
            tokio::spawn(async move {
                on_exec_approval_response(event_id, rx, conversation).await;
            });
        }
        _ => {}
    }
}
```

```rust
// DO: Conservative default to Denied on failure; submit decision to conversation.
async fn on_patch_approval_response(
    event_id: String,
    receiver: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    conversation: Arc<CodexConversation>,
) {
    let value = match receiver.await {
        Ok(v) => v,
        Err(err) => {
            error!("request failed: {err:?}");
            let _ = conversation
                .submit(Op::PatchApproval { id: event_id.clone(), decision: ReviewDecision::Denied })
                .await;
            return;
        }
    };

    let response = serde_json::from_value::<ApplyPatchApprovalResponse>(value).unwrap_or_else(|err| {
        error!("failed to deserialize ApplyPatchApprovalResponse: {err}");
        ApplyPatchApprovalResponse { decision: ReviewDecision::Denied }
    });

    if let Err(err) = conversation
        .submit(Op::PatchApproval { id: event_id, decision: response.decision })
        .await
    {
        error!("failed to submit PatchApproval: {err}");
    }
}
```

```rust
// DO: Typed parse with Denied fallback; note request-failure path currently logs only.
async fn on_exec_approval_response(
    event_id: String,
    receiver: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    conversation: Arc<CodexConversation>,
) {
    let value = match receiver.await {
        Ok(v) => v,
        Err(err) => {
            tracing::error!("request failed: {err:?}");
            return; // Consider aligning with patch flow + timeout in follow-ups.
        }
    };

    let response = serde_json::from_value::<ExecCommandApprovalResponse>(value).unwrap_or_else(|err| {
        error!("failed to deserialize ExecCommandApprovalResponse: {err}");
        ExecCommandApprovalResponse { decision: ReviewDecision::Denied }
    });

    if let Err(err) = conversation
        .submit(Op::ExecApproval { id: event_id, decision: response.decision })
        .await
    {
        error!("failed to submit ExecApproval: {err}");
    }
}
```

```rust
// DO: Use constants and typed params in wire format; skip optional fields when None.
pub const APPLY_PATCH_APPROVAL_METHOD: &str = "applyPatchApproval";
pub const EXEC_COMMAND_APPROVAL_METHOD: &str = "execCommandApproval";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ApplyPatchApprovalParams {
    pub conversation_id: ConversationId,
    pub file_changes: HashMap<PathBuf, FileChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_root: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ExecCommandApprovalParams {
    pub conversation_id: ConversationId,
    pub command: Vec<String>,
    pub cwd: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
```

```rust
// DO: Rename to ClientRequest and alias MCP’s to avoid clashes.
use crate::wire_format::ClientRequest;
use mcp_types::ClientRequest as McpClientRequest;

if let Ok(request_json) = serde_json::to_value(request.clone())
    && let Ok(codex_request) = serde_json::from_value::<ClientRequest>(request_json)
{
    self.codex_message_processor.process_request(codex_request).await;
    return;
}

let request_id = request.id.clone();
let client_request = match McpClientRequest::try_from(request) {
    Ok(req) => req,
    Err(e) => { self.respond_error(request_id, INSUFFICIENT_REQUEST_ERROR_CODE, format!("Failed to convert request: {e}")); return; }
};
```

**DON’Ts**
- Don’t block the main loop waiting on approvals; avoid `.await` inline—spawn a task instead.
- Don’t rely on ad‑hoc JSON notifications long‑term; migrate toward a typed, stable notification enum.
- Don’t move an `Event` you still need; clone it before serialization or reuse.
- Don’t hardcode method strings; don’t diverge from `APPLY_PATCH_APPROVAL_METHOD` / `EXEC_COMMAND_APPROVAL_METHOD`.
- Don’t accept untyped responses; always deserialize into `ApplyPatchApprovalResponse` / `ExecCommandApprovalResponse`.
- Don’t ignore errors without a decision path; ensure the conversation gets a decision (ideally Denied on failures).
- Don’t conflate request directions; don’t reuse the old `CodexRequest` name—use `ClientRequest` and `ServerRequest`.
- Don’t let approval tasks live forever; don’t skip timeouts on spawned waits.
- Don’t omit required context; don’t send approval requests without a `conversation_id`.
- Don’t bypass idiomatic types; don’t use raw strings where `PathBuf`, `Vec<String>`, or `HashMap<PathBuf, FileChange>` are intended.