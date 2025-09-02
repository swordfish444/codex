**DOs**
- Destructure without cloning: move or borrow the `id`; clone only when needed.
```rust
// Good: rename and move out of `event` without cloning
let Event { id: event_id, msg } = event;

// Later, if you truly need an owned copy, clone at the use site:
persist_task_id(event_id.clone());

// If only borrowing is needed:
use_id(&event_id);
```

- Add a blank line after early returns: make non‑linear control flow stand out.
```rust
if should_drop_streaming {
    return;
}

self.request_redraw();
```

- Surface user‑critical events even for stale task IDs: always show approvals and errors; only update “running” UI when it’s the active task.
```rust
// Before handling the message:
if self.should_drop_streaming_for(&event_id, &msg) {
    return;
}

match msg {
    EventMsg::Error(ErrorEvent { message }) => {
        if self.active_task_id.as_deref() == Some(event_id.as_str()) {
            self.bottom_pane.set_task_running(false);
            self.active_task_id = None;
        }
        self.conversation_history.add_error(message);
    }
    EventMsg::ExecApprovalRequest(req) => {
        // Always surface approval dialogs
        self.bottom_pane.show_exec_approval(event_id.clone(), req);
    }
    EventMsg::ApplyPatchApprovalRequest(req) => {
        self.bottom_pane.show_patch_approval(event_id.clone(), req);
    }
    _ => { /* … */ }
}
```

- Centralize stale‑stream filtering: keep the match arms clean by gating once.
```rust
fn should_drop_streaming_for(&self, event_id: &str, msg: &EventMsg) -> bool {
    // Never drop user‑critical events:
    if matches!(msg,
        EventMsg::Error(_)
        | EventMsg::ExecApprovalRequest(_)
        | EventMsg::ApplyPatchApprovalRequest(_)
    ) {
        return false;
    }

    // Drop late streaming text for non‑active tasks:
    self.active_task_id
        .as_deref()
        .is_some_and(|active| active != event_id)
}

// Usage in handler:
let Event { id: event_id, msg } = event;
if self.should_drop_streaming_for(&event_id, &msg) {
    return;
}

match msg { /* no per‑arm drop checks needed */ }
```

**DON’Ts**
- Don’t clone IDs preemptively: avoid `let event_id = event.id.clone();` unless ownership is required immediately.
```rust
// Avoid:
let event_id = event.id.clone();
let Event { id: _, msg } = event;

// Prefer:
let Event { id: event_id, msg } = event; // move once, clone only at use site if needed
```

- Don’t hide control‑flow changes: skipping the blank line after an early return makes code harder to scan.
```rust
// Harder to read:
if should_drop_streaming {
    return;
}
self.request_redraw();

// Better: add a spacer line (see DOs).
```

- Don’t drop approval requests for stale tasks: users must see prompts to proceed or understand blocks.
```rust
// Avoid:
if should_drop_streaming {
    return; // This would suppress approval dialogs — not okay.
}

// Do instead: whitelist approvals in `should_drop_streaming_for`.
```

- Don’t copy/paste gating logic into most match arms: consolidate the check to one place.
```rust
// Avoid repetitive per‑arm checks:
match msg {
    EventMsg::AgentMessage(_) => {
        if should_drop_streaming { return; }
        /* … */
    }
    EventMsg::AgentMessageDelta(_) => {
        if should_drop_streaming { return; }
        /* … */
    }
    // …
}

// Do: a single pre‑match guard or a helper function (see DOs).
```