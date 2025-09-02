**DOs**
- **Document boolean returns**: Explain exactly what `true`/`false` mean and what the caller must do.
```rust
impl ChatWidget<'_> {
    /// Handle Ctrl+C.
    /// Returns true if the caller should exit the app.
    /// Returns false if handled internally (interrupt or hint shown).
    pub(crate) fn on_ctrl_c(&mut self) -> bool { /* ... */ }
}
```

- **Delegate exit to the app**: Let the widget decide intent; the app performs the exit.
```rust
// codex-rs/tui/src/app.rs
if widget.on_ctrl_c() {
    // Widget signaled exit
    let _ = self.app_event_tx.send(AppEvent::ExitRequest);
}
```

- **Interrupt only when a task is running**: Forward `Op::Interrupt` if busy; never exit in this branch.
```rust
// codex-rs/tui/src/chatwidget.rs
if self.bottom_pane.is_task_running() {
    self.bottom_pane.clear_ctrl_c_quit_hint();
    self.submit_op(Op::Interrupt);
    return false; // handled internally
}
```

- **Show a quit hint when idle; exit on second press**: First Ctrl+C shows “Ctrl+C to quit”; second Ctrl+C exits.
```rust
// Still in on_ctrl_c()
if self.bottom_pane.ctrl_c_quit_hint_visible() {
    return true; // caller should exit
} else {
    self.bottom_pane.show_ctrl_c_quit_hint();
    return false; // handled internally
}
```

- **Clear the hint on other activity**: Any keypress or task start should remove the hint.
```rust
// On any key event
pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) {
    self.bottom_pane.clear_ctrl_c_quit_hint();
    /* ...existing handling... */
}

// When a task starts
EventMsg::TaskStarted => {
    self.bottom_pane.clear_ctrl_c_quit_hint();
    self.bottom_pane.set_task_running(true);
    self.request_redraw();
}
```

- **Update UI through dedicated APIs and request redraws**: Keep state changes localized and visible.
```rust
// codex-rs/tui/src/bottom_pane/mod.rs
pub(crate) fn show_ctrl_c_quit_hint(&mut self) {
    self.ctrl_c_quit_hint = true;
    self.composer.set_ctrl_c_quit_hint(true, self.has_input_focus);
    self.request_redraw();
}

pub(crate) fn clear_ctrl_c_quit_hint(&mut self) {
    if self.ctrl_c_quit_hint {
        self.ctrl_c_quit_hint = false;
        self.composer.set_ctrl_c_quit_hint(false, self.has_input_focus);
        self.request_redraw();
    }
}
```

- **Keep responsibilities layered**: BottomPane owns hint state; Composer renders it; Widget orchestrates behavior; App decides process exit.
```rust
// codex-rs/tui/src/bottom_pane/chat_composer.rs (rendering)
let bs = if has_focus {
    if self.ctrl_c_quit_hint {
        BlockState { right_title: "Ctrl+C to quit".into(), border_style: Style::default() }
    } else {
        BlockState { right_title: "Enter to send | Ctrl+D to quit | Ctrl+J for newline".into(),
                     border_style: Style::default() }
    }
} else { /* ... */ };
```

**DON'Ts**
- **Don’t leave boolean semantics implicit**: Undocumented booleans invite misuse.
```rust
// BAD: What does true mean here?
pub(crate) fn on_ctrl_c(&mut self) -> bool { /* ... */ }
```

- **Don’t exit immediately on first Ctrl+C when idle**: Show the hint first to prevent accidental exits.
```rust
// BAD: Immediate exit on first Ctrl+C
if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
    let _ = self.app_event_tx.send(AppEvent::ExitRequest);
}
```

- **Don’t send interrupts when idle**: Only interrupt when work is active.
```rust
// BAD: Unconditionally interrupting
self.submit_op(Op::Interrupt); // will do nothing useful when idle
```

- **Don’t forget to clear the hint on activity**: Otherwise the UI will exit on next Ctrl+C unexpectedly.
```rust
// BAD: No hint clearing at the start of key handling
pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) {
    /* ...handles keys but leaves hint visible... */
}
```

- **Don’t mutate UI state without a redraw**: Users won’t see the updated hint/title.
```rust
// BAD: State changed, but no redraw requested
self.ctrl_c_quit_hint = true;
self.composer.set_ctrl_c_quit_hint(true, self.has_input_focus);
// missing: self.request_redraw();
```