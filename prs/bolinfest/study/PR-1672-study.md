# PR #1672 Review Takeaways — Easily Selectable History (bolinfest)

## DOs

- Keep the protocol boundary clean: move UI-only types out of `protocol.rs`.
  - Example: put `FinalOutput` in `codex-common` (or similar), not `core/protocol`.
  ```rust
  // codex-common/src/final_output.rs
  use std::fmt;
  use codex_core::protocol::TokenUsage;
  use serde::{Serialize, Deserialize};

  #[derive(Debug, Clone, Deserialize, Serialize)]
  pub struct FinalOutput {
      pub token_usage: TokenUsage,
      pub final_message: Option<String>,
  }

  impl From<TokenUsage> for FinalOutput {
      fn from(token_usage: TokenUsage) -> Self {
          Self { token_usage, final_message: None }
      }
  }

  impl fmt::Display for FinalOutput {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
          let u = &self.token_usage;
          write!(
              f,
              "Token usage: total={} input={}{} output={}{}",
              u.total_tokens,
              u.input_tokens,
              u.cached_input_tokens
                  .map(|c| format!(" (cached {c})"))
                  .unwrap_or_default(),
              u.output_tokens,
              u.reasoning_output_tokens
                  .map(|r| format!(" (reasoning {r})"))
                  .unwrap_or_default()
          )
      }
  }
  ```
  ```rust
  // codex-rs/cli/src/main.rs
  let usage = codex_tui::run_main(tui_cli, codex_linux_sandbox_exe)?;
  println!("{}", codex_common::FinalOutput::from(usage));
  ```

- Use doc comments and attach them to items.
  ```rust
  /// Buffers assistant response deltas until a final message arrives.
  /// This avoids partial-line rendering in scrollback.
  struct ChatWidget<'a> {
      /// Accumulates streamed assistant text.
      answer_buffer: String,
      // ...
  }
  ```

- Keep comments current; remove stale references.
  ```rust
  // Before: mentions a removed design doc
  /// … hybrid scrollback model described in `fix-history-plan.md`.

  // After: describe the behavior without referencing non-existent docs
  /// Appends immutable history above the inline viewport.
  ```

- DRY repetitive “append last history” code with a helper.
  ```rust
  impl ChatWidget<'_> {
      fn emit_last_history_entry(&mut self) {
          if let Some(lines) = self.conversation_history.last_entry_plain_lines() {
              self.app_event_tx.send(AppEvent::InsertHistory(lines));
          }
      }

      fn submit_user_message(&mut self, msg: String) {
          self.conversation_history.add_user_message(msg);
          self.emit_last_history_entry();
      }
  }
  ```

- Encapsulate wrapping/normalization logic in a small struct (not closures).
  ```rust
  // tui/src/insert_history.rs (outline)
  struct LineBuilder { term_width: usize, spans: Vec<Span<'static>>, width: usize }

  impl LineBuilder {
      fn new(term_width: usize) -> Self { Self { term_width, spans: vec![], width: 0 } }
      fn push_word(&mut self, word: &mut String, style: Style, out: &mut Vec<Line<'static>>) { /* … */ }
      fn consume_whitespace(&mut self, ws: &mut String, style: Style, out: &mut Vec<Line<'static>>) { /* … */ }
      fn flush_line(&mut self, out: &mut Vec<Line<'static>>) { /* … */ }
  }
  ```

- Normalize and wrap text before `Terminal::insert_before`.
  ```rust
  pub(crate) fn insert_history_lines(terminal: &mut tui::Tui, lines: Vec<Line<'static>>) {
      let term_width = terminal.size().map(|a| a.width).unwrap_or(80) as usize;
      let mut physical: Vec<Line<'static>> = Vec::new();

      for logical in lines {
          let mut b = LineBuilder::new(term_width);
          let mut ws = String::new();

          for span in logical.spans {
              let style = span.style;
              let mut word = String::new();
              for ch in span.content.chars() {
                  if ch == '\n' { b.push_word(&mut word, style, &mut physical); ws.clear(); b.flush_line(&mut physical); continue; }
                  if ch.is_whitespace() { b.push_word(&mut word, style, &mut physical); ws.push(ch); }
                  else { b.consume_whitespace(&mut ws, style, &mut physical); word.push(ch); }
              }
              b.push_word(&mut word, style, &mut physical);
          }
          if !b.spans.is_empty() { physical.push(Line::from(std::mem::take(&mut b.spans))); }
          else { physical.push(Line::from(Vec::<Span<'static>>::new())); }
      }

      let total = physical.len() as u16;
      terminal.insert_before(total, |buf| {
          for (i, line) in physical.into_iter().enumerate() {
              Paragraph::new(line).render(Rect { x: 0, y: i as u16, width: buf.area.width, height: 1 }, buf);
          }
      }).ok();
  }
  ```

- Preserve or clearly indicate unsupported outputs (e.g., images).
  ```rust
  match cell {
      HistoryCell::CompletedMcpToolCallWithImageOutput { .. } => vec![
          Line::from("tool result (image output omitted)"),
          Line::from(""),
      ],
      _ => view.lines.clone(),
  }
  ```

- Keep docs and config in sync when removing features.
  ```rust
  // core/src/config_types.rs
  #[derive(Deserialize, Debug, Clone, PartialEq, Default)]
  pub struct Tui {} // remove disable_mouse_capture here

  // config.md
  [tui]
  # (mouse capture option removed to match code)
  ```

- Remove noisy/unnecessary comments.
  ```rust
  // Before
  use crate::tui; // for the Tui type alias

  // After
  use crate::tui;
  ```

## DON’Ts

- Don’t put UI/CLI glue types in `core/src/protocol.rs`.
  ```rust
  // Avoid
  // core/src/protocol.rs
  #[derive(Debug)] pub struct FinalOutput { /* UI concern */ }
  ```

- Don’t leave obsolete references in comments.
  ```rust
  // Avoid: referring to deleted docs or plans
  /// See `fix-history-plan.md` for details.
  ```

- Don’t use `//` where `///` doc comments are appropriate.
  ```rust
  // Avoid
  // Buffers assistant response deltas.

  // Prefer
  /// Buffers assistant response deltas.
  ```

- Don’t stream partial fragments that cause truncation/flicker; flush on full message or at newline boundaries.
  ```rust
  // Avoid: re-rendering on every tiny delta
  self.answer_buffer.push_str(&delta);
  self.conversation_history.replace_prev_agent_message(&self.config, self.answer_buffer.clone());

  // Prefer: flush only complete lines (or final)
  self.answer_buffer.push_str(&delta);
  if let Some(idx) = self.answer_buffer.rfind('\n') {
      let complete = self.answer_buffer[..=idx].to_string();
      self.conversation_history.add_agent_message(&self.config, complete);
      self.emit_last_history_entry();
      self.answer_buffer = self.answer_buffer[idx+1..].to_string();
  }
  ```

- Don’t assume `Paragraph::wrap` will apply during `insert_before`; perform explicit wrapping.
  ```rust
  // Avoid: relying on implicit wrap with insert_before
  Paragraph::new(line).wrap(Wrap { trim: true });

  // Prefer: pre-wrap into single-row Lines, then render each on its own row
  ```

- Don’t duplicate formatting/printing logic across binaries; centralize via `Display`.
  ```rust
  // Avoid
  println!("{}", format!("Token usage: total={}", usage.total_tokens));

  // Prefer
  println!("{}", codex_common::FinalOutput::from(usage));
  ```

- Don’t leave half-removed features in code or docs.
  ```rust
  // Avoid: keeping `disable_mouse_capture` in docs while code ignores it
  ```

- Don’t use large, stateful inline closures for formatting/flow; use small functions or structs.
  ```rust
  // Avoid
  let flush_word = |word: &mut String, spans: &mut Vec<Span<'static>>, width: &mut usize, /* … */| { /* … */ };

  // Prefer
  struct LineBuilder { /* … */ }
  impl LineBuilder { fn push_word(&mut self, /* … */) { /* … */ } }
  ```