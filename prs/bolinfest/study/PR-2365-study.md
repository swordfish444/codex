**DOs**
- **Emit `TurnAborted` on interrupt:** Send `EventMsg::TurnAborted` in response to `Op::Interrupt`, carrying a reason.
```rust
// core/src/codex.rs
let event = Event {
    id: self.sub_id,
    msg: EventMsg::TurnAborted(TurnAbortedEvent { reason }),
};
let tx_event = self.sess.tx_event.clone();
tokio::spawn(async move { let _ = tx_event.send(event).await; });
```

- **Use explicit abort reasons:** Distinguish between user interrupts and task replacement.
```rust
// protocol/src/protocol.rs
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnAbortedEvent { pub reason: TurnAbortReason }

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnAbortReason { Interrupted, Replaced }
```

- **Abort with correct reason in session:** Replace `abort()` calls with `abort(TurnAbortReason::...)`.
```rust
// Replacing a running task
if let Some(current) = state.current_task.take() {
    current.abort(TurnAbortReason::Replaced);
}

// Handling an interrupt
fn interrupt_task(&self) {
    info!("interrupt received: abort current task, if any");
    let mut state = self.state.lock_unchecked();
    state.pending_approvals.clear();
    state.pending_input.clear();
    if let Some(task) = state.current_task.take() {
        task.abort(TurnAbortReason::Interrupted);
    }
}
```

- **Route interrupts via session entry points:** Call `sess.interrupt_task()` for all interrupt-like ops.
```rust
match sub.op {
    Op::Interrupt => { sess.interrupt_task(); }
    Op::ExecApproval { decision: ReviewDecision::Abort, .. } => { sess.interrupt_task(); }
    Op::PatchApproval { decision: ReviewDecision::Abort, .. } => { sess.interrupt_task(); }
    _ => { /* existing handling */ }
}
```

- **Defer MCP interrupt responses until `TurnAborted`:** Queue pending requests and reply when the event arrives, including the reason.
```rust
// mcp-server: record pending interrupts
{
    let mut map = self.pending_interrupts.lock().await;
    map.entry(conversation_id.0).or_default().push(request_id);
}
let _ = conversation.submit(Op::Interrupt).await;

// On TurnAborted, respond to all pending
match msg {
    EventMsg::TurnAborted(ev) => {
        let pending = {
            let mut map = pending_interrupts.lock().await;
            map.remove(&conversation_id.0).unwrap_or_default()
        };
        let response = InterruptConversationResponse { abort_reason: ev.reason };
        for rid in pending { outgoing.send_response(rid, response.clone()).await; }
    }
    _ => {}
}
```

- **Expose abort reason on the wire:** Extend the interrupt response payload to include `abort_reason`.
```rust
// mcp-server/src/wire_format.rs
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InterruptConversationResponse {
    pub abort_reason: TurnAbortReason,
}
```

- **Handle `TurnAborted` in all consumers:** Provide clear, user-facing messages.
```rust
// exec event processor
match event.msg {
    EventMsg::TurnAborted(ev) => match ev.reason {
        TurnAbortReason::Interrupted => ts_println!(self, "task interrupted"),
        TurnAbortReason::Replaced => ts_println!(self, "task aborted: replaced by a new task"),
    },
    _ => {}
}

// TUI chat widget
EventMsg::TurnAborted(_) => self.on_error("Turn interrupted".to_owned()),
```

- **Update protocol docs/comments accordingly:** Note that `Op::Interrupt` yields `EventMsg::TurnAborted`.
```rust
// protocol/src/protocol.rs
/// Abort current task.
/// This server sends [`EventMsg::TurnAborted`] in response.
Interrupt,
```

- **Adjust tests to expect the event:** Wait for a `turn_aborted` notification instead of an immediate response/error.
```rust
// mcp-server/tests/interrupt.rs
let _turn_aborted = timeout(
    DEFAULT_READ_TIMEOUT,
    mcp_process.read_stream_until_notification_message("turn_aborted"),
).await??;
```

**DON'Ts**
- **Don’t emit `EventMsg::Error` for interrupts:** Use `EventMsg::TurnAborted` with a precise `TurnAbortReason`.
```rust
// ❌ Old
msg: EventMsg::Error(ErrorEvent { message: " Turn interrupted".to_string() })

// ✅ New
msg: EventMsg::TurnAborted(TurnAbortedEvent { reason: TurnAbortReason::Interrupted })
```

- **Don’t reply immediately to MCP interrupt requests:** Wait for `TurnAborted` before sending `InterruptConversationResponse`.
```rust
// ❌ Old: respond right away
outgoing.send_response(request_id, InterruptConversationResponse {}).await;

// ✅ New: respond after event with reason (see DOs snippet)
```

- **Don’t drop sessions without cleanup:** Ensure `Drop` triggers `interrupt_task()` to clear state and abort current work.
```rust
impl Drop for Session {
    fn drop(&mut self) {
        self.interrupt_task();
    }
}
```

- **Don’t assume a single pending interrupt:** Support multiple queued `RequestId`s per conversation.
```rust
// Use a Vec<RequestId> in a HashMap keyed by conversation UUID
pending_interrupts: Arc<Mutex<HashMap<Uuid, Vec<RequestId>>>>
```

- **Don’t short-circuit event flow in loops:** Avoid stray `continue;` that can suppress downstream handling.
```rust
// ❌ Avoid early `continue;` after handling an event
match event.msg {
    EventMsg::ExecApprovalRequest(_) => {
        // handle...
        // no `continue;` here
    }
    _ => {}
}
```

- **Don’t treat `TurnAborted` as a no-op in processors:** Add explicit match arms in exec/TUI instead of ignoring it.
```rust
// ❌ Missing arm: `_ => {}` only
// ✅ Include `EventMsg::TurnAborted(_)` arm (see DOs)
```