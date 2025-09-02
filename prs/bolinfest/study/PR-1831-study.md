**PR 1831 Review — PatchApplyEnd Handling: DOs and DON'Ts**

**DOs**
- Handle `PatchApplyEnd`: add a clear failure entry; ignore success.
```rust
match event {
    EventMsg::PatchApplyEnd(PatchApplyEndEvent { call_id, stdout, stderr, success }) => {
        if !success {
            self.add_to_history(HistoryCell::new_patch_failed_event(stdout, stderr));
        }
        // No history entry when success == true
    }
    _ => {}
}
```

- Preserve context from `PatchApplyBegin`: keep `changes` by `call_id` for better failure UI.
```rust
// In struct:
patch_changes: std::collections::HashMap<u64, HashMap<PathBuf, FileChange>>,

// On begin:
EventMsg::PatchApplyBegin(PatchApplyBeginEvent { call_id, changes }) => {
    self.patch_changes.insert(call_id, changes.clone());
    self.add_to_history(HistoryCell::PendingPatch { view: TextBlock::from_changes(&changes) });
}

// On end (failure):
EventMsg::PatchApplyEnd(PatchApplyEndEvent { call_id, stdout, stderr, success }) => {
    if !success {
        let changes = self.patch_changes.remove(&call_id);
        self.add_to_history(HistoryCell::new_patch_failed_event_with_changes(call_id, stdout, stderr, changes));
    }
}
```

- Omit empty `stdout`/`stderr` lines to reduce noise.
```rust
pub fn new_patch_failed_event(stdout: String, stderr: String) -> Self {
    let mut lines = vec!["patch failed".red().bold().into()];
    if !stdout.is_empty() { lines.push(stdout.into()); }
    if !stderr.is_empty() { lines.push(stderr.into()); }
    lines.push("".into());
    HistoryCell::PatchFailed { view: TextBlock::new(lines) }
}
```

- Use `Stylize` helpers and basic `into()` spans for TUI text.
```rust
use ratatui::prelude::Stylize;
// Good:
let header: Line = "patch failed".red().bold().into();
let msg: Line = "apply failed on workspace".dim().into();
```

- Inline variables with `format!` when constructing messages.
```rust
let msg = format!("patch failed (call {call_id})");
let line: Line = msg.red().bold().into();
```

**DON'Ts**
- Don’t leave protocol events unhandled (no “TODO” or silent drops).
```rust
// Bad: event silently ignored
// match event { _ => {} }
```

- Don’t render empty `stdout`/`stderr` placeholders.
```rust
// Bad: pushes empty lines that clutter UI
let lines = vec![
    "patch failed".red().bold().into(),
    stdout.into(), // may be ""
    stderr.into(), // may be ""
];
```

- Don’t discard begin/end linkage; use `call_id` to join events.
```rust
// Bad: throws away call_id, making it hard to correlate
EventMsg::PatchApplyEnd(PatchApplyEndEvent { call_id: _, .. }) => { /* ... */ }
```

- Don’t create success entries for `PatchApplyEnd`.
```rust
// Bad: success should be silent
if success {
    self.add_to_history(HistoryCell::new_info("patch applied")); // avoid
}
```

- Don’t hand-build styles via `Span::styled` when `Stylize` suffices.
```rust
// Bad
// Line::from(Span::styled("patch failed", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));

// Good
let line: Line = "patch failed".red().bold().into();
```