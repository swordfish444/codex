**DOs**

- **Structure Event Loop Cleanly**: Own per-conversation state in a `Conversation` and run a cancellable background loop. Handle Ok events in a helper; break on Err.
```rust
use tokio_util::sync::CancellationToken;

pub(crate) fn spawn_conversation_loop(conv: Arc<Conversation>) {
    tokio::spawn(async move {
        let cancel = conv.cancel.clone();
        let codex = conv.codex.clone();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                res = codex.next_event() => match res {
                    Ok(event) => conv.handle_event(event).await,
                    Err(e) => {
                        tracing::error!("next_event error (session {}): {e}", conv.session_id);
                        break;
                    }
                }
            }
        }
    });
}
```

- **Provide Explicit Cancellation**: Use a token (or equivalent) and disable streaming on cancellation; route `notifications/cancelled` using the original tool request.
```rust
// Store original params so cancel knows what to do.
self.tool_request_map.lock().await.insert(id.clone(), params.clone());

// Typed METHOD from the crate, not a string.
use mcp_types::CancelledNotification;
mcp.send_notification(CancelledNotification::METHOD, Some(json!({ "requestId": req_id }))).await?;

// For ConversationStream cancel:
stream_conversation::handle_cancel(self, &args).await;

// For ConversationSendMessage cancel:
let codex = self.conversation_map.lock().await.get(&args.conversation_id.0).unwrap().codex();
codex.submit(codex_core::protocol::Op::Interrupt).await?;
```

- **Use `submit()` (Not `submit_with_id()`)**: Let Codex assign IDs unless you can prove uniqueness.
```rust
use codex_core::protocol::{Op, InputItem};

conversation.codex().submit(Op::UserInput { items: vec![InputItem::Text { text }] }).await?;
```

- **Centralize `RequestId` Conversion**: Avoid copy/paste; use a helper.
```rust
// codex-rs/mcp-server/src/request_id.rs
pub(crate) fn request_id_to_string(id: &mcp_types::RequestId) -> String {
    match id { mcp_types::RequestId::String(s) => s.clone(), mcp_types::RequestId::Integer(i) => i.to_string() }
}
// usage
let rid = crate::request_id::request_id_to_string(&request_id);
```

- **Minimize Lock Scopes; Never `await` While Holding Locks**: Decide under lock; await outside. Push to buffers under the same lock path when not awaiting.
```rust
// Decide under lock
let should_stream = { self.state.lock().await.streaming_enabled };

if should_stream {
    // No lock held here
    handle_exec_approval_request(...).await;
} else {
    // Lock once to mutate state; no awaits
    let mut st = self.state.lock().await;
    st.pending_elicitations.push(PendingElicitation::ExecRequest(req));
}
```

- **Define Clear Snapshot Semantics**: If buffering full history, document it; emit an initial snapshot on connect and then live events.
```rust
pub(crate) async fn set_streaming(&self, enabled: bool) {
    if enabled {
        let (events, pending) = {
            let mut st = self.state.lock().await;
            st.streaming_enabled = true;
            (st.buffered_events.clone(), std::mem::take(&mut st.pending_elicitations))
        };
        self.emit_initial_state_with(events).await;
        self.drain_pending_elicitations_from(pending).await;
    } else {
        self.state.lock().await.streaming_enabled = false;
    }
}
```

- **Use Typed Notifications Where Possible**: Prefer a `ServerNotification` enum and a typed sender for codegen friendliness.
```rust
// Build and send typed notification
let note = ServerNotification::InitialState(InitialStateNotificationParams {
    meta: Some(NotificationMeta { conversation_id: Some(ConversationId(self.session_id)), request_id: None }),
    initial_state: InitialStatePayload { events },
});
self.outgoing.send_server_notification(note).await;
```

- **Log With Context-Rich Messages**: Include session/conversation IDs and the concrete error payload.
```rust
tracing::error!("Failed to serialize InitialState (session {}): {err:?}", self.session_id);
tracing::error!("Unexpected SessionConfigured (session {}): {:?}", self.session_id, ev);
```

- **Prefer Top-Level Spawners**: Keep `spawn_*` as free functions for clarity and testability.
```rust
pub(crate) fn spawn_conversation_loop(conv: Arc<Conversation>) { /* ... */ }
```

- **Deduplicate Test Helpers and Use Typed METHODS**: Centralize config helpers; use `CancelledNotification::METHOD`; add bounded timeouts.
```rust
// tests/common/config.rs
pub fn create_config_toml(home: &Path, uri: &str) -> std::io::Result<()> { /* ... */ }

// In tests
use mcp_types::CancelledNotification;
mcp.send_notification(CancelledNotification::METHOD, Some(json!({ "requestId": id }))).await?;
let note = timeout(Duration::from_secs(3), mcp.read_stream_until_notification_method("agent_message")).await??;
```


**DON’Ts**

- **Don’t Block The Event Loop On User Elicitations**: Never wait for approval responses inside the loop.
```rust
// 🚫 Wrong: blocks loop and may deadlock
match event.msg {
    EventMsg::ApplyPatchApprovalRequest(req) => {
        // Holding state or loop context while awaiting
        handle_patch_approval_request(...).await; // ← blocks loop
    }
    _ => {}
}
```

- **Don’t Hardcode Notification Method Strings**: Use typed constants or enums.
```rust
// 🚫 Wrong
outgoing.send_custom_notification("notifications/cancelled", params);

// ✅ Right
use mcp_types::CancelledNotification;
mcp.send_notification(CancelledNotification::METHOD, Some(params)).await?;
```

- **Don’t Recreate `RequestId` Stringification Everywhere**: Centralize it.
```rust
// 🚫 Wrong
let id = match &request.id { RequestId::String(s) => s.clone(), RequestId::Integer(i) => i.to_string() };

// ✅ Right
let id = request_id_to_string(&request.id);
```

- **Don’t Hold Locks Across `await` Or Double-Lock Needlessly**: Avoid back-to-back locks; never `.await` while locked.
```rust
// 🚫 Wrong
let mut st = self.state.lock().await;
if st.streaming_enabled { /* ... */ }
drop(st);
let mut st = self.state.lock().await; // immediate re-lock

// ✅ Right
let streaming = { self.state.lock().await.streaming_enabled };
if !streaming {
    self.state.lock().await.pending_elicitations.push(item);
} else {
    handle_exec_approval_request(...).await;
}
```

- **Don’t Use `submit_with_id()` Without Guaranteed Uniqueness**: Prefer `submit()`.
```rust
// 🚫 Wrong
codex.submit_with_id(Submission { id: user_supplied_id, op }).await?;

// ✅ Right
codex.submit(op).await?;
```

- **Don’t Swallow Error Details**: Log the payload and context.
```rust
// 🚫 Wrong
tracing::error!("Codex runtime error");

// ✅ Right
tracing::error!("Codex error (session {}): {:?}", self.session_id, err);
```

- **Don’t Duplicate Test Utilities Or Use Stringly-Typed Methods**: Keep helpers in `tests/common`; rely on typed METHODs and bounded waits.
```rust
// 🚫 Wrong
mcp.send_notification("notifications/cancelled", Some(json!({ "requestId": id }))).await?;

// ✅ Right
use mcp_types::CancelledNotification;
mcp.send_notification(CancelledNotification::METHOD, Some(json!({ "requestId": id }))).await?;
```

- **Don’t Leave Stray/Redundant Comments**: Remove “clone once outside the loop”-style notes once the code is self-evident.

- **Don’t Bury Core Concepts In Only One Layer**: Long-term, consider moving durable abstractions (like Conversation/history semantics) into `codex-core` with thin wrappers in the MCP server.