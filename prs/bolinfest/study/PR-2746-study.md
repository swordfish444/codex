**DOs**
- Trim before emitting events: Ensure history is trimmed before constructing/sending the “task complete” event to avoid race conditions.
```rust
sess.remove_task(&sub_id);

{
    let mut state = sess.state.lock_unchecked();
    state.history.keep_last_messages(1);
} // lock dropped here

let event = /* build Event using sub_id */;
sess.send_event(event).await;
```

- Limit lock scope: Use a block so the mutex guard is dropped before any await.
```rust
{
    let mut state = sess.state.lock_unchecked();
    state.history.keep_last_messages(1);
} // guard out of scope before .await
// safe to await afterward
sess.send_event(event).await;
```

- Keep ordering deterministic: Remove task → trim history → build/send event.
```rust
sess.remove_task(&sub_id);
{ let mut s = sess.state.lock_unchecked(); s.history.keep_last_messages(1); }
let event = /* build Event */;
sess.send_event(event).await;
```

- Inline one‑off helpers: If trimming is only used here, prefer inlining over adding a new API.
```rust
// Good: inline once
{
    let mut state = sess.state.lock_unchecked();
    state.history.keep_last_messages(1);
}
```

**DON’Ts**
- Don’t emit before trimming: Sending the event first reintroduces flakiness.
```rust
// Bad: event goes out with stale history
sess.remove_task(&sub_id);
let event = /* build Event */;
sess.send_event(event).await;           // emitted too early
let mut state = sess.state.lock_unchecked();
state.history.keep_last_messages(1);    // happens too late
```

- Don’t hold locks across awaits: Avoid awaiting while a mutex guard is alive.
```rust
// Bad: guard lives across .await
let mut state = sess.state.lock_unchecked();
state.history.keep_last_messages(1);
sess.send_event(event).await; // holding lock here can deadlock or stall
```

- Don’t add a one‑use method: Skip expanding the Session API for single‑site logic.
```rust
// Avoid adding this if called once:
impl Session {
    fn trim_history_to_last_messages(&self, n: usize) {
        let mut state = self.state.lock_unchecked();
        state.history.keep_last_messages(n);
    }
}
// And avoid this call when inlining is sufficient:
sess.trim_history_to_last_messages(1);
```

- Don’t leave trailing trims: Remove any post‑send history trimming that used to follow event emission.
```rust
// Remove this legacy tail (incorrect placement):
let event = /* build Event */;
sess.send_event(event).await;
let mut state = sess.state.lock_unchecked();
state.history.keep_last_messages(1); // should have happened earlier
```