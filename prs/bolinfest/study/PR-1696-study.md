**PR 1696 — Review Takeaways (bolinfest)**

**DOs**
- **Use enums for cancellation**: Return a small, self-documenting enum (not bool) for Ctrl-C handling across views.
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancellationEvent { Ignored, Handled }

pub(crate) trait BottomPaneView<'a> {
    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        CancellationEvent::Ignored
    }
}
```

- **Centralize Ctrl-C routing**: Let `BottomPane` delegate Ctrl-C to the active view; show the quit hint when handled.
```rust
pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
    let mut view = match self.active_view.take() {
        Some(v) => v,
        None => return CancellationEvent::Ignored,
    };

    let event = view.on_ctrl_c(self);
    match event {
        CancellationEvent::Handled => {
            if !view.is_complete() {
                self.active_view = Some(view);
            } else if self.is_task_running {
                self.active_view = Some(Box::new(StatusIndicatorView::new(self.app_event_tx.clone())));
            }
            self.show_ctrl_c_quit_hint();
        }
        CancellationEvent::Ignored => {
            self.active_view = Some(view);
        }
    }
    event
}
```

- **Treat Ctrl-C like Esc in modals**: Abort the request and clear the queue in the approval modal.
```rust
fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
    self.current.on_ctrl_c();
    self.queue.clear();
    CancellationEvent::Handled
}
```

- **Print approval-needed commands before the modal**: Add a history entry first; inline variables with `format!`.
```rust
let cmdline = strip_bash_lc_and_escape(&command);
let text = format!(
    "command requires approval:\n$ {cmdline}{reason}",
    reason = reason.as_ref().map(|r| format!("\n{r}")).unwrap_or_default()
);
self.conversation_history.add_background_event(text);
self.emit_last_history_entry();
self.conversation_history.scroll_to_bottom();
self.bottom_pane.push_approval_request(request);
self.request_redraw();
```

- **Record approval decisions (with feedback) in history**: Build lines once; append optional feedback after the match.
```rust
let mut lines: Vec<Line<'static>> = match &self.approval_request {
    ApprovalRequest::Exec { command, .. } => {
        let cmd = strip_bash_lc_and_escape(command);
        vec![
            Line::from("approval decision"),
            Line::from(format!("$ {cmd}")),
            Line::from(format!("decision: {decision:?}")),
        ]
    }
    ApprovalRequest::ApplyPatch { .. } => vec![
        Line::from(format!("patch approval decision: {decision:?}")),
    ],
};

if !feedback.trim().is_empty() {
    lines.push(Line::from("feedback:"));
    lines.extend(feedback.lines().map(|l| Line::from(l.to_string())));
}
lines.push(Line::from(""));
self.app_event_tx.send(AppEvent::InsertHistory(lines));
```

- **Prefer early returns to reduce nesting**: Use match + early return instead of deep if/else chains.
```rust
let Some(mut view) = self.active_view.take() else {
    return CancellationEvent::Ignored;
};
```

- **Keep `mpsc` receivers alive in tests**: Retain `rx` for the test’s scope; assert on enums.
```rust
let (tx, _rx) = channel::<AppEvent>(); // keep _rx in scope
let tx = AppEventSender::new(tx);
let mut pane = BottomPane::new(BottomPaneParams { app_event_tx: tx, has_input_focus: true });
pane.push_approval_request(exec_request());
assert_eq!(CancellationEvent::Handled, pane.on_ctrl_c());
assert!(pane.ctrl_c_quit_hint_visible());
assert_eq!(CancellationEvent::Ignored, pane.on_ctrl_c());
```

- **Propagate enum up the stack**: Make `ChatWidget::on_ctrl_c` return `CancellationEvent`.
```rust
pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
    match self.bottom_pane.on_ctrl_c() {
        CancellationEvent::Handled => return CancellationEvent::Handled,
        CancellationEvent::Ignored => {}
    }
    if self.bottom_pane.is_task_running() {
        self.bottom_pane.clear_ctrl_c_quit_hint();
        self.submit_op(Op::Interrupt);
        self.answer_buffer.clear();
        self.reasoning_buffer.clear();
        CancellationEvent::Ignored
    } else if self.bottom_pane.ctrl_c_quit_hint_visible() {
        self.submit_op(Op::Shutdown);
        CancellationEvent::Handled
    } else {
        self.bottom_pane.show_ctrl_c_quit_hint();
        CancellationEvent::Ignored
    }
}
```

**DON’Ts**
- **Don’t return raw bools for cancellation**: Avoid `bool` for “handled or not”; use a `CancellationEvent` enum instead.

- **Don’t build strings piecemeal**: Avoid `push_str` chains when `format!` with inlined placeholders is clearer and safer.
```rust
// Prefer:
let msg = format!("command requires approval:\n$ {cmdline}{reason}", reason = opt_reason);
// Over:
let mut msg = String::new();
msg.push_str("command requires approval:\n$ ");
msg.push_str(&cmdline);
```

- **Don’t drop test receivers**: Don’t hide channel creation in helpers that drop `rx`; construct `(tx, _rx)` inline and keep `_rx` alive.

- **Don’t duplicate feedback handling**: Don’t repeat “append feedback lines” in each match arm; do it once after building the common lines.

- **Don’t lose view state after Ctrl-C**: Don’t forget to put the view back when ignored, or to switch to `StatusIndicatorView` when the modal completes and a task is running.

- **Don’t forget UI updates**: Don’t omit `request_redraw()` after enqueueing modal requests, or the Ctrl-C quit hint after a handled cancellation.