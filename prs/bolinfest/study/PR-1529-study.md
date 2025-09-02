**DOs**

- Use `format!` with raw strings and inline variables into `{}`.
```rust
let prompt = format!(
    r#"This chat continues a previous conversation.
After providing the summary, acknowledge that /compact was applied.

Here is the summary:

{}"#,
    summary.trim()
);
```

- Keep “important first”: define `App` and primary fns before helpers.
```rust
impl App<'_> {
    fn dispatch_codex_event(&mut self, event: Event) {
        if let Some(p) = &mut self.pending_summarization {
            self.handle_summarization_response(event, p);
            return;
        }
        if let AppState::Chat { widget } = &mut self.app_state {
            widget.handle_codex_event(event);
        }
    }
}

// Helper lives after the caller.
fn handle_summarization_response(&mut self, event: Event, pending: &mut PendingSummarization) { /* ... */ }
```

- Bring long paths into scope with `use` for readability.
```rust
use codex_core::protocol::EventMsg;

match &event.msg {
    EventMsg::AgentMessage(msg) => { /* ... */ }
    EventMsg::TaskComplete(done) => { /* ... */ }
    _ => {}
}
```

- Keep event-loop slim; route logic to helpers like `dispatch_codex_event()`.
```rust
match app_event {
    AppEvent::CodexEvent(e) => self.dispatch_codex_event(e),
    AppEvent::RequestRedraw => self.pending_redraw = true,
    _ => {}
}
```

- Gate summarization collection to the specific summarize task, not all messages.
```rust
// When submitting the op, record the task id.
let task_id = widget.submit_op(Op::SummarizeContext);
self.pending_summarization = Some(PendingSummarization {
    task_id,
    started_receiving: false,
    buffer: String::new(),
});

// Later, only collect messages for that task.
if let (Some(p), EventMsg::AgentMessage(msg)) = (&mut self.pending_summarization, &event.msg) {
    if event.task_id == Some(p.task_id) {
        p.started_receiving = true;
        p.buffer.push_str(&msg.message);
        p.buffer.push('\n');
    }
}
```

- Match enum and UI ordering by frequency; place `/compact` right after `/new` and mirror in descriptions.
```rust
#[derive(EnumIter, Clone, Copy)]
pub enum SlashCommand {
    New,
    Compact,
    Diff,
    Quit,
    ToggleMouseMode,
}

impl SlashCommand {
    pub fn description(self) -> &'static str {
        match self {
            SlashCommand::New => "Start a new chat.",
            SlashCommand::Compact => "Summarize and compact the current conversation to free up context.",
            SlashCommand::Diff => "Show git diff of the working directory (including untracked files)",
            SlashCommand::Quit => "Exit the application.",
            SlashCommand::ToggleMouseMode => "Toggle mouse mode (enable for scrolling, disable for text selection)",
        }
    }
}
```

- Use consistent, punctuated comments that follow existing style.
```rust
/// Tracks pending summarization requests for the compact feature.
struct PendingSummarization {
    task_id: TaskId,
    started_receiving: bool,
    buffer: String,
}
```

- Favor integration tests that validate behavior end-to-end over brittle unit tests.
```rust
#[test]
fn compact_flow_creates_new_chat_with_summary() {
    // Arrange: create app, enter chat state with history.
    // Act: trigger `/compact`, simulate summarize task events, complete.
    // Assert: new ChatWidget is created and initial prompt contains the summary.
}
```

**DON’Ts**

- Don’t stash `"{}"` in a const and call `.replace("{}", ...)`.
```rust
// ❌ Avoid
const TEMPLATE: &str = "Summary:\n\n{}";
let prompt = TEMPLATE.replace("{}", summary);
```

- Don’t declare helpers and supporting structs before the main types.
```rust
// ❌ Avoid putting this before `App`
fn create_compact_summary_prompt(..) -> String { /* ... */ }
```

- Don’t handle summarization inline inside the deepest part of the event loop.
```rust
// ❌ Avoid deep, inlined logic here
AppEvent::CodexEvent(event) => {
    if let EventMsg::AgentMessage(msg) = &event.msg { /* ... */ }
}
```

- Don’t match long, repetitive paths instead of importing the type.
```rust
// ❌ Avoid
match &event.msg {
    codex_core::protocol::EventMsg::AgentMessage(m) => { /* ... */ }
    _ => {}
}
```

- Don’t treat every `AgentMessage` as part of the summary.
```rust
// ❌ Avoid collecting all messages
if let EventMsg::AgentMessage(m) = &event.msg {
    pending.buffer.push_str(&m.message);
}
```

- Don’t alphabetize slash commands; keep them ordered by expected frequency.
```rust
// ❌ Avoid: purely alphabetical ordering
enum SlashCommand { Compact, Diff, New, Quit, ToggleMouseMode }
```

- Don’t keep low-signal tests that mirror implementation details or string internals.
```rust
// ❌ Avoid tests like:
assert!(COMPACT_SUMMARY_TEMPLATE.contains("{}"));
```

- Don’t omit periods in doc comments; match the repository’s comment style.
```rust
// ❌ Avoid missing punctuation
/// Tracks pending summarization requests
```