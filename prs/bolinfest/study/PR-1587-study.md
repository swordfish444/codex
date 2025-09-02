**DOs**
- **Disable Flaky Tests Properly**: Use `#[ignore]` with a clear reason and keep doc comments truthful.
```rust
/// Verifies the agent retries when SSE ends before `response.completed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore] // flaky: race condition; tracked in GH-1234
async fn retries_on_early_close() {
    // test body...
}
```

- **Process SSE Incrementally**: Handle events as they arrive rather than buffering everything first.
```rust
// Good: act on events as they stream in
while let Some(Ok(event)) = stream.next().await {
    match event {
        // handle events...
        _ => {}
    }
}
```

- **Wire Delta Events End‑to‑End**: Add protocol types, parse SSE fields, and forward deltas through the system.
```rust
// protocol.rs
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentMessageDeltaEvent { pub delta: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningDeltaEvent { pub delta: String }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum EventMsg {
    AgentMessageDelta(AgentMessageDeltaEvent),
    AgentReasoningDelta(AgentReasoningDeltaEvent),
    // ...
}

// client.rs (SSE -> ResponseEvent)
match event_type.as_str() {
    "response.output_text.delta" => {
        if let Some(delta) = event.delta { tx_event.send(Ok(ResponseEvent::OutputTextDelta(delta))).await.ok(); }
    }
    "response.reasoning_summary_text.delta" => {
        if let Some(delta) = event.delta { tx_event.send(Ok(ResponseEvent::ReasoningSummaryDelta(delta))).await.ok(); }
    }
    _ => {}
}
```

- **Ignore Deltas During Aggregation**: Skip deltas when the consumer expects a final, aggregated output.
```rust
match Pin::new(self).poll_next(cx) {
    Poll::Ready(Some(Ok(ResponseEvent::OutputTextDelta(_))))
    | Poll::Ready(Some(Ok(ResponseEvent::ReasoningSummaryDelta(_)))) => {
        // aggregation waits for OutputItemDone
        continue;
    }
    _ => {}
}
```

- **Write Precise Test Assertions**: Assert on exact lines or structured outputs, not naive substring counts.
```rust
let hi_lines = stdout.lines().filter(|line| line.trim() == "hi").count();
assert_eq!(hi_lines, 1, "Expected exactly one line with 'hi'");
```

- **Handle New Variants Explicitly Across Consumers**: Update `exec`, `mcp-server`, and `tui` match arms even if only to TODO.
```rust
match event.msg {
    EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta: _ }) => {
        // TODO: decide CLI streaming UX
    }
    EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta: _ }) => {
        // TODO: decide CLI streaming UX
    }
    _ => {}
}
```

- **Right‑Size Channels (Prefer Constants/Config)**: Justify capacity changes and make them easy to tune.
```rust
const RESPONSE_EVENT_CAP: usize = 1600; // large bursts during delta streaming
let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(RESPONSE_EVENT_CAP);
```

**DON’Ts**
- **Don’t Commit Large Blocks of Commented‑Out Code**: Remove or revert; don’t leave dead code in the tree.
```rust
// BAD: commented-out file contents
// //! Verifies that the agent retries when...
// use std::time::Duration;
// #[tokio::test] async fn ... {}
```

- **Don’t Buffer the Entire Stream Before Acting**: This risks timeouts and hides streaming regressions.
```rust
// BAD: buffer first, act later
let mut input = Vec::new();
while let Some(event) = stream.next().await {
    input.push(event?);
}
for event in input { /* ... */ }
```

- **Don’t Swallow New Events With Catch‑Alls**: Add explicit variants so future changes aren’t silently ignored.
```rust
// BAD: hides new protocol messages
match event {
    _ => { /* ignored */ }
}

// GOOD: enumerate and handle (or TODO)
match event {
    EventMsg::AgentMessageDelta(_) => { /* TODO */ }
    EventMsg::AgentReasoningDelta(_) => { /* TODO */ }
    _ => {}
}
```

- **Don’t Leave Context‑Free TODOs**: Include what/why and a tracking reference so cleanup is actionable.
```rust
// BAD
// TODO: support this

// GOOD
// TODO(GH-1234): Stream deltas in TUI; buffer per message ID and repaint incrementally.
```

- **Don’t Mix Unrelated or Mistaken Merges**: If a file was merged by mistake, revert it instead of commenting it out.
```bash
# Revert an unintended file change cleanly
git checkout origin/main -- codex-rs/core/tests/stream_no_completed.rs
```