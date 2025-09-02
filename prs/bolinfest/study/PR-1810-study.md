**DOs**

- Bold, Exact Assertions: prefer `assert_eq!` over loose contains checks; use pretty diffs for failures.
  ```
  use pretty_assertions::assert_eq;

  assert_eq!(
      items,
      vec![ResponseItem::Message {
          id: None,
          role: "assistant".to_string(),
          content: vec![ContentItem::OutputText { text: "Hello, world!".to_string() }],
      }]
  );
  ```

- Merge Duplicates In History: combine adjacent assistant messages and test the exact merged result.
  ```
  // merge logic
  match (&*item, history.items.last_mut()) {
      (ResponseItem::Message { role: r1, content: c1, .. },
       Some(ResponseItem::Message { role: r2, content: c2, .. }))
          if r1 == "assistant" && r2 == "assistant" => append_text_content(c2, c1),
      _ => history.items.push(item.clone()),
  }
  ```

- Use Destructuring To Ignore Fields: make intent clear when final payload is unused due to streaming.
  ```
  match event {
      EventMsg::AgentMessage(AgentMessageEvent { message: _ }) => {
          self.finalize_stream(StreamKind::Answer);
      }
      _ => {}
  }
  ```

- Centralize Redraws And Control Flow: track whether a branch handled the update, then redraw once.
  ```
  pub fn update_status_text(&mut self, text: String) {
      let mut handled = false;
      if let Some(view) = self.active_view.as_mut() {
          handled |= matches!(view.update_status_text(text.clone()), NeedsRedraw);
      } else {
          let mut v = StatusIndicatorView::new(self.app_event_tx.clone());
          v.update_text(text.clone());
          self.active_view = Some(Box::new(v));
          self.status_view_active = true;
          handled = true;
      }
      if !handled {
          self.live_status.get_or_insert_with(|| StatusIndicatorWidget::new(self.app_event_tx.clone()))
              .update_text(text);
      }
      self.request_redraw();
  }
  ```

- Import Types At Top: avoid fully-qualified paths in signatures.
  ```
  use ratatui::text::Line;

  pub fn set_live_ring_rows(&mut self, max_rows: u16, rows: Vec<Line<'static>>) { ... }
  ```

- Prefer &'static str When Possible: avoid allocating `String` for static content.
  ```
  lines.push(ratatui::text::Line::from(""));
  ```

- Name Things Clearly: use descriptive names instead of one-letter variables.
  ```
  if let Some(view) = &self.active_view {
      view.render(view_rect, buf);
  }
  ```

- Make Test Utilities Reusable: factor repeated setup into helpers or a scenario struct.
  ```
  struct TestScenario { width: u16, height: u16, term: Terminal<TestBackend> }

  impl TestScenario {
      fn run_insert(&mut self, lines: Vec<Line<'static>>) -> Vec<u8> { ... }
      fn screen_rows_from_bytes(&self, bytes: &[u8]) -> Vec<String> { ... }
  }
  ```

- Gate Emulator Tests And Keep Them Dev-Only: put vt100 under `dev-dependencies` and behind a feature.
  ```
  # Cargo.toml
  [features]
  vt100-tests = []

  [dev-dependencies]
  vt100 = { version = "0.16.2", optional = true }
  pretty_assertions = "1"
  ```

- Use Writer-Based Output For Testability: separate rendering from I/O, then test via a buffer.
  ```
  pub fn insert_history_lines_to_writer<B, W>(
      term: &mut Terminal<B>,
      writer: &mut W,
      lines: Vec<Line>,
  ) where
      B: ratatui::backend::Backend,
      W: std::io::Write,
  { /* queue!(writer, ...); */ }
  ```

- Handle Unicode Width Correctly: wrap using display width (emoji/CJK) and provide invariance tests.
  ```
  pub fn take_prefix_by_width(text: &str, max_cols: usize) -> (String, &str, usize) { ... }

  let (prefix, _suffix, width) = take_prefix_by_width("😀你好", 4);
  assert_eq!(prefix, "😀"); // 😀 is width 2
  assert_eq!(width, 2);
  ```

- Strip ANSI Before Measuring Or Animating: prevent control bytes from corrupting rendering.
  ```
  let line = ansi_escape_line(&self.text);
  let plain = line.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
  ```

- Be Explicit About Channel Backpressure And Logs: pick bounded vs. unbounded intentionally and log at the right level.
  ```
  // backpressure on submissions; allow bursts on events
  let (tx_sub, rx_sub) = async_channel::bounded(64);
  let (tx_event, rx_event) = async_channel::unbounded();

  debug!("Configuring session: model={model}; provider={provider:?}; resume={resume_path:?}");
  ```

- Keep UI Layout Invariants Tested: assert bottom padding behavior and overlay stacking.
  ```
  // height=2: top has status, bottom is blank padding
  assert!(row0.contains("Working"));
  assert!(row1.trim().is_empty());
  ```

- Clean Module Visibility For Tests: make internal modules `pub` where practical within the workspace (e.g., `custom_terminal`, `insert_history`, `live_wrap`).


**DON’Ts**

- Don’t Use Vague Tests: avoid `assert!(contains(...))` when you can assert the exact structure or full screen state.

- Don’t Scatter `request_redraw()` Across Branches: avoid multiple early returns that obscure flow; consolidate redraw at the end.

- Don’t Keep Emulator Crates In Runtime Deps: keep `vt100` in `[dev-dependencies]` and guard tests with `#[cfg(feature = "vt100-tests")]`.

- Don’t Leave Single-Letter Identifiers: replace `ov`, `s`, etc., with clear names like `view`, `text`.

- Don’t Hardcode Fully Qualified Types In Sigs: import once at the module top.

- Don’t Allocate Strings For Literals: use `&'static str` where possible.

- Don’t Leave Unused Code/Events/Comments: remove unused variants (e.g., stale `AppEvent`), “Removed …” comments, or unclear references (e.g., “TS ref?”) without context.

- Don’t Emit Raw ANSI Into Buffers: always sanitize before measuring or rendering to avoid cursor jumps/artifacts.

- Don’t Forget To Clear Live Overlays: call `clear_live_ring()` and reset `status_view_active` when tasks complete.

- Don’t Duplicate Assistant Messages: merge adjacent assistant entries in conversation history.

- Don’t Overuse `info!` For Noisy Logs: prefer `debug!` for verbose or frequent messages.

- Don’t Handwave Backpressure: document why a channel is bounded/unbounded and choose deliberately.

- Don’t Leave Padding/Height Edge Cases Untested: small heights should gracefully shrink padding while preserving essential content.


**Code Snippets Recap**

- Unified status update with single redraw:
  ```
  pub fn update_status_text(&mut self, text: String) {
      let mut handled = false;
      if let Some(view) = self.active_view.as_mut() {
          handled |= matches!(view.update_status_text(text.clone()), NeedsRedraw);
      } else {
          let mut v = StatusIndicatorView::new(self.app_event_tx.clone());
          v.update_text(text.clone());
          self.active_view = Some(Box::new(v));
          self.status_view_active = true;
          handled = true;
      }
      if !handled {
          self.live_status.get_or_insert_with(|| StatusIndicatorWidget::new(self.app_event_tx.clone()))
              .update_text(text);
      }
      self.request_redraw();
  }
  ```

- Dev-only emulator tests gating:
  ```
  // tests/vt100_history.rs
  #![cfg(feature = "vt100-tests")]
  #![expect(clippy::expect_used)]

  use ratatui::backend::TestBackend;
  use ratatui::layout::Rect;
  use ratatui::text::Line;

  struct TestScenario { /* ... */ }

  #[test]
  fn hist_001_basic_insertion_no_wrap() {
      let area = Rect::new(0, 5, 20, 1);
      let mut scenario = TestScenario::new(20, 6, area);
      let buf = scenario.run_insert(vec![Line::from("first"), Line::from("second")]);
      let rows = scenario.screen_rows_from_bytes(&buf);
      assert_eq!(rows[4], "first");
      assert_eq!(rows[5], "second");
  }
  ```

- History merging test with exact expectation:
  ```
  let mut h = ConversationHistory::default();
  h.record_items([&assistant_msg("Hello"), &assistant_msg(", world!")]);
  assert_eq!(
      h.contents(),
      vec![ResponseItem::Message {
          id: None,
          role: "assistant".to_string(),
          content: vec![ContentItem::OutputText { text: "Hello, world!".to_string() }],
      }]
  );
  ```

- Writer-based insertion API usage:
  ```
  let mut out: Vec<u8> = Vec::new();
  insert_history_lines_to_writer(&mut terminal, &mut out, lines);
  // feed `out` into vt100::Parser for assertions
  ```

- Unicode-aware wrapping helper:
  ```
  pub fn take_prefix_by_width(text: &str, max_cols: usize) -> (String, &str, usize) {
      use unicode_width::UnicodeWidthChar;
      let mut cols = 0;
      let mut end = 0;
      for (i, ch) in text.char_indices() {
          let w = UnicodeWidthChar::width(ch).unwrap_or(0);
          if cols + w > max_cols { break; }
          cols += w;
          end = i + ch.len_utf8();
          if cols == max_cols { break; }
      }
      (text[..end].to_string(), &text[end..], cols)
  }
  ```