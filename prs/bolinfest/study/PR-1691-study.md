**DOs**
- Bold invariants: keep `codex exec` approval-free.
  ```rust
  // In event_processor_with_human_output.rs
  match event.msg {
      EventMsg::ExecApprovalRequest(_) => {
          // Unreachable for `codex exec`; do nothing.
          CodexStatus::Continue
      }
      _ => { /* existing handling */ }
  }
  ```

- Pair tests with real fixes: change the widget layout so command/options remain visible even with long reasons.
  ```rust
  // In user_approval_widget.rs render()
  use ratatui::{layout::{Constraint, Direction, Layout}, widgets::Paragraph};

  let chunks = Layout::default()
      .direction(Direction::Vertical)
      .constraints([
          Constraint::Length(1), // title
          Constraint::Min(1),    // command (wraps)
          Constraint::Length(1), // cwd
          Constraint::Length(1), // options
          Constraint::Min(0),    // reason (clipped/scrollable)
      ])
      .split(area);

  // Render command and options first so they can’t be pushed off-screen.
  Paragraph::new(shell_join(&command)).render(chunks[1], buf);
  Paragraph::new(format!("cwd: {}", cwd.display())).render(chunks[2], buf);
  Paragraph::new("Yes (y)  No (n)  Details (d)").render(chunks[3], buf);

  // Reason is last; it will clip when space runs out.
  Paragraph::new(reason_text).wrap(ratatui::widgets::Wrap { trim: true }).render(chunks[4], buf);
  ```

- Clamp verbose text: cap reason height so it never hides critical lines.
  ```rust
  // Compute max lines available for the reason block.
  let reserved = 1 + 1 + 1; // title + cwd + options (command is separate and must be visible)
  let max_reason_lines = area.height.saturating_sub(reserved + 1); // +1 for command line

  // Truncate the reason to fit. (Simple, allocation-free line clamp.)
  let mut reason = reason_text.lines();
  let visible_reason = reason.by_ref().take(max_reason_lines as usize).collect::<Vec<_>>().join("\n");

  Paragraph::new(visible_reason).wrap(ratatui::widgets::Wrap { trim: true }).render(chunks[4], buf);
  ```

- Keep output aligned with product mode: only print approval prompts in flows that actually request approvals.
  ```rust
  match event.msg {
      EventMsg::ApplyPatchApprovalRequest(ev) => {
          // Valid in interactive approval flows; show summary here.
          ts_println!(self, "approval required for apply_patch:");
          for (path, change) in &ev.changes {
              println!("  {} {}", format_file_change(change).style(self.cyan), path.to_string_lossy());
          }
          CodexStatus::InitiateShutdown
      }
      _ => CodexStatus::Continue,
  }
  ```

- Use concise formatting and styling per conventions.
  ```rust
  // Prefer inline variables in format! and Stylize helpers.
  use ratatui::style::Stylize;
  ts_println!(self, "{} {}", "approval required for".magenta(), shell_join(&command).bold());
  ```

**DON’Ts**
- Don’t surface approval prompts in `codex exec` or change its control flow.
  ```rust
  // ❌ Avoid adding printing + shutdown for Exec approvals in the event processor.
  EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent { command, cwd, reason, .. }) => {
      ts_println!(self, "approval required for {} in {}", shell_join(&command), cwd.display());
      if let Some(r) = reason { ts_println!(self, "{r}"); }
      return CodexStatus::InitiateShutdown; // <- don’t do this for `codex exec`
  }
  ```

- Don’t ship tests without the behavior change that makes them pass meaningfully.
  ```rust
  // ❌ Tests-only change that asserts visibility without modifying layout:
  #[test]
  fn exec_command_is_visible_in_small_viewport() {
      // ... renders ...
      assert!(rendered.contains("echo 123 && printf 'hello'"));
  }
  // Add the layout/clamper logic that ensures this is actually true at runtime.
  ```

- Don’t let long reasons push core content out of view.
  ```rust
  // ❌ Rendering reason first can hide the command/options in small viewports.
  lines.push(reason_paragraph);   // long, multi-line
  lines.push(command_paragraph);  // gets clipped off-screen
  lines.push(options_paragraph);  // gets clipped off-screen
  ```

- Don’t duplicate approval handling across layers; respect existing gating.
  ```rust
  // ❌ Event processor shouldn’t reintroduce approval prompts for modes
  // that already disable them at the CLI/config layer.
  ```

- Don’t ignore style and formatting conventions when printing.
  ```rust
  // ❌ Verbose/indirect formatting:
  ts_println!(self, "{} {}", "approval required for".to_string(), shell_join(&command).to_string());

  // ✅ Inline variables; no unnecessary to_string():
  ts_println!(self, "approval required for {}", shell_join(&command));
  ```