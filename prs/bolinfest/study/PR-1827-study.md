**DOs**

- **Use a dedicated event variant:** Represent expected interruptions with a specific, structured event (not an error), so higher layers decide presentation.
```rust
// Good: add a new structured variant
pub enum EventMsg {
    // ...
    TurnInterrupted(TurnInterruptedEvent),
    // ...
}

// Emitting it
self.handle.abort();
let event = Event {
    id: self.sub_id,
    msg: EventMsg::TurnInterrupted(TurnInterruptedEvent { /* fields as needed */ }),
};
let _ = self.sess.tx_event.send(event).await;
```

- **Let the UI choose how to render:** Handle the structured variant in the UI without string checks; update UI state there.
```rust
// Good: UI decides presentation for structured event
match event.msg {
    EventMsg::TurnInterrupted(_) => {
        self.add_to_history(HistoryCell::new_background_event("Response interrupted.".into()));
        self.bottom_pane.set_task_running(false);
        self.request_redraw();
    }
    _ => {}
}
```

- **Preserve event-loop semantics:** Keep the receive/forward loop stable; don’t introduce ad‑hoc breaks based on particular events.
```rust
// Good: forward all events; exit only on channel error or stream end
res = codex.next_event() => match res {
    Ok(event) => {
        debug!("Received event: {event:?}");
        if tx.send(event).is_err() {
            error!("Error sending event");
            break;
        }
        // No special-case break here
    }
    Err(e) => {
        debug!("Event stream ended: {e:?}");
        break;
    }
}
```

- **Downgrade severity for expected conditions:** Use `debug!` (or lower) when the stream ends in normal/expected scenarios.
```rust
// Good: normal termination is not an error
Err(e) => {
    debug!("Event stream ended: {e:?}");
    break;
}
```


**DON’Ts**

- **Don’t emit errors for expected interrupts:** Avoid `EventMsg::Error` for user-initiated cancels or routine shutdowns.
```rust
// Bad: misclassifies an expected case as an error
let event = Event {
    id: self.sub_id,
    msg: EventMsg::Error(ErrorEvent { message: "Turn interrupted".into() }),
};
```

- **Don’t drive UI logic by string matching:** Avoid brittle checks like “contains('Turn interrupted')”.
```rust
// Bad: brittle UI logic
if message.contains("Turn interrupted") {
    self.bottom_pane.set_task_running(false);
}
```

- **Don’t change control flow by special-casing events:** Breaking on `ShutdownComplete` (or similar) can be unsafe without a broader audit.
```rust
// Bad: ad-hoc control-flow change
if matches!(event.msg, EventMsg::ShutdownComplete) {
    break; // may skip required drains/cleanups
}
```

- **Don’t log normal endings as errors:** Reserve `error!` for genuine failures, not expected stream termination.
```rust
// Bad: noisy logging for a normal condition
Err(e) => {
    error!("Error receiving event: {e:?}");
    break;
}
```