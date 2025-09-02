**DOs**
- Boldly handle both streaming and non‑streaming paths: print headers on first delta, flush each chunk, and still render the final non‑delta message/reasoning if no deltas arrived.
```rust
match event {
    EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
        if !self.answer_started {
            ts_println!(self, "{}\n", "codex".style(self.italic).style(self.magenta));
            self.answer_started = true;
        }
        print!("{delta}");
        #[allow(clippy::expect_used)]
        std::io::stdout().flush().expect("could not flush stdout");
    }
    EventMsg::AgentMessage(AgentMessageEvent { message }) => {
        if !self.answer_started {
            ts_println!(self, "{}\n{}", "codex".style(self.italic).style(self.magenta), message);
        } else {
            println!();
            self.answer_started = false;
        }
    }
    _ => {}
}
```

- Stream “thinking” only when enabled and use the correct label; mirror the answer flow for reasoning.
```rust
EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta }) => {
    if self.show_agent_reasoning {
        if !self.reasoning_started {
            ts_println!(self, "{}\n", "thinking".style(self.italic).style(self.magenta));
            self.reasoning_started = true;
        }
        print!("{delta}");
        #[allow(clippy::expect_used)]
        std::io::stdout().flush().expect("could not flush stdout");
    }
}
EventMsg::AgentReasoning(AgentReasoningEvent { text }) => {
    if self.show_agent_reasoning {
        if !self.reasoning_started {
            ts_println!(self, "{}\n{}", "thinking".style(self.italic).style(self.magenta), text);
        } else {
            println!();
            self.reasoning_started = false;
        }
    }
}
```

- Flush stdout on every streamed chunk to keep the CLI/TUI responsive.
```rust
print!("{delta}");
#[allow(clippy::expect_used)]
std::io::stdout().flush().expect("could not flush stdout");
```

- Prefer if/else over early returns inside large match arms to keep control flow easy to reason about.
```rust
if !self.answer_started {
    ts_println!(self, "{}\n{}", "codex".style(self.italic).style(self.magenta), message);
} else {
    println!();
    self.answer_started = false;
}
```

- Keep `use` statements at the top (or fully qualify) instead of sprinkling imports inside match arms.
```rust
// Top of file:
use std::io::Write;

// or fully qualify where used:
std::io::stdout().flush().expect("could not flush stdout");
```

- Inline variables directly in formatting macros.
```rust
println!("tokens used: {total_tokens}");
ts_println!(self, "command: {cmd}");
```

- Implement streaming in the TUI with buffers and “replace last” semantics.
```rust
// Agent message streaming
EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
    if self.answer_buffer.is_empty() {
        self.conversation_history.add_agent_message(&self.config, "".to_string());
    }
    self.answer_buffer.push_str(&delta);
    self.conversation_history.replace_prev_agent_message(&self.config, self.answer_buffer.clone());
    self.request_redraw();
}
EventMsg::AgentMessage(AgentMessageEvent { message }) => {
    if self.answer_buffer.is_empty() {
        self.conversation_history.add_agent_message(&self.config, message);
    } else {
        self.conversation_history.replace_prev_agent_message(&self.config, message);
        self.answer_buffer.clear();
    }
    self.request_redraw();
}
```

- When replacing the last rendered cell, recompute height and set `0` if width is unknown.
```rust
let width = self.cached_width.get();
let height = if width > 0 { entry.cell.height(width) } else { 0 };
entry.line_count.set(height);
```

- Place throttling/debounce at the top-level TUI event loop (not inside widgets), and never throttle user-typed input redraws.
```rust
// Event loop sketch
match app_event {
    AppEvent::Key(_) => {
        needs_immediate_redraw = true;
        draw_now();
    }
    AppEvent::Redraw => {
        if needs_immediate_redraw || last_draw.elapsed() >= Duration::from_millis(16) {
            draw_now();
            needs_immediate_redraw = false;
        } else {
            schedule_coalesced_redraw();
        }
    }
    _ => {}
}
```

- Keep code/comments concise and accurate; remove redundant doc comments on obvious fields like `answer_started` and `reasoning_started`.

- Finish streamed answers cleanly: print a newline and reset flags to prepare for the next message.
```rust
println!();
self.answer_started = false;
```

- Maintain small style fixes that improve clarity (e.g., ASCII apostrophes in user-facing text, no stray trailing commas in macro calls).
```rust
ts_println!(self, "{}\n", "codex".style(self.italic).style(self.magenta));
```

**DON’Ts**
- Don’t rely solely on deltas; non‑streaming responses must still render final messages/reasoning.
```rust
// Missing final AgentMessage handler means non-streaming prints nothing — avoid this.
```

- Don’t use early `return` inside match arms when a simple if/else suffices.
```rust
// Avoid
if !self.answer_started {
    ts_println!(self, "{}\n{}", "codex".style(self.italic).style(self.magenta), message);
    return;
}
println!();
```

- Don’t `use std::io::Write;` inside a match arm or branch.
```rust
// Avoid
use std::io::Write; // nested inside logic
```

- Don’t throttle redraws in widgets like `ChatWidget`; doing so can drop visible updates and feel laggy.
```rust
// Avoid throttling here:
fn request_redraw(&mut self) {
    if Instant::now().duration_since(self.last_redraw_time) > Duration::from_millis(100) {
        self.app_event_tx.send(AppEvent::Redraw);
    }
}
```

- Don’t leave misleading or redundant comments (e.g., “else, we rerender one last time” when there’s no exclusive else).
```rust
// Avoid comments that restate code or mislead the reader.
```

- Don’t use the wrong label for reasoning vs. answers (“thinking” vs. “codex”).
```rust
// Avoid
ts_println!(self, "{}\n", "codex".style(self.italic).style(self.magenta)); // for reasoning
```

- Don’t forget to guard reasoning output behind `show_agent_reasoning`.
```rust
// Avoid
print!("{delta}"); // without checking self.show_agent_reasoning
```

- Don’t ignore zero width when recomputing layout; not setting `line_count` can break scrolling and rendering.
```rust
// Avoid
entry.line_count.set(entry.cell.height(width)); // width may be 0
```