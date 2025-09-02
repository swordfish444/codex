**DOs**

- Color-code headers and status: Use Stylize for clear, scannable results.
  ```rust
  let title_line = Line::from(vec![
      "tool".magenta(),
      " ".into(),
      if success { "success".green() } else { "failed".red() },
      format!(", duration: {duration}").gray(),
  ]);
  ```

- Render invocations with Line/Span and concise styling: Server/tool in blue, args in gray.
  ```rust
  let invocation = Line::from(vec![
      server.as_str().blue(),
      ".".into(),
      tool.as_str().blue(),
      "(".into(),
      args_str.as_str().gray(),
      ")".into(),
  ]);
  ```

- Keep active/completed formatting consistent: Reuse the same invocation line in both states.
  ```rust
  // Active
  view.push(Line::from(vec!["tool".magenta(), " running...".dim()]));
  view.push(invocation.clone());

  // Completed
  view.push(title_line);
  view.push(invocation);
  ```

- Render CallToolResultContent items individually: Text gets compact-JSON + truncation; others get clear placeholders; errors are styled.
  ```rust
  match result {
    Ok(mcp_types::CallToolResult { content, .. }) if !content.is_empty() => {
      view.push("".into());
      for c in content {
        let line = match c {
          CallToolResultContent::TextContent(t) =>
            format_and_truncate_tool_result(&t.text, TOOL_CALL_MAX_LINES, num_cols as usize),
          CallToolResultContent::ImageContent(_) => "<image content>".to_string(),
          CallToolResultContent::AudioContent(_) => "<audio content>".to_string(),
          CallToolResultContent::EmbeddedResource(r) =>
            format!("embedded resource: {}", r.resource.uri()),
        };
        view.push(Line::from(line).gray());
      }
      view.push("".into());
    }
    Err(e) => view.push(Line::from(vec!["Error: ".red().bold(), e.into()])),
    _ => {}
  }
  ```

- Truncate by grapheme clusters (unicode-segmentation), not bytes/chars: Add unit tests for edge cases.
  ```rust
  pub(crate) fn truncate_text(text: &str, max_graphemes: usize) -> String { /* … */ }

  #[test] fn truncates_with_ellipsis() {
    assert_eq!(truncate_text("Hello, world!", 8), "Hello...");
  }
  ```

- Format JSON as one line with spaces to aid Ratatui wrapping: Prefer a helper over PrettyFormatter defaults.
  ```rust
  if let Some(s) = format_json_compact(raw_text) {
      truncate_text(&s, budget);
  }
  ```

- Move formatting helpers into a module and test them: Keep widgets lean and focused on layout.
  ```rust
  // src/text_formatting.rs
  pub(crate) fn format_and_truncate_tool_result(..) -> String { /* … */ }
  pub(crate) fn format_json_compact(..) -> Option<String> { /* … */ }
  pub(crate) fn truncate_text(..) -> String { /* … */ }

  // lib.rs
  mod text_formatting;
  ```

- Name width parameters for what they are: Use num_cols rather than terminal_width.
  ```rust
  pub(crate) fn new_completed_mcp_tool_call(
      num_cols: u16,
      invocation: Line<'static>,
      start: Instant,
      success: bool,
      result: Result<mcp_types::CallToolResult, String>,
  ) -> Self { /* … */ }
  ```

- Preserve JSON key order for predictable output and tests: Enable serde_json preserve_order.
  ```toml
  # Cargo.toml (alpha-sorted)
  regex-lite = "0.1"
  serde_json = { version = "1", features = ["preserve_order"] }
  shlex = "1.3.0"
  tui-textarea = "0.7.0"
  unicode-segmentation = "1.12.0"
  uuid = "1"
  ```

- Use format! with inline variables: Keep strings concise and readable.
  ```rust
  let msg = format!("embedded resource: {uri}");
  let label = format!(", duration: {duration}");
  ```

- Keep review hygiene tight: After rebases, scan diffs for accidental deletions and validate behavior with tests.
  ```bash
  git diff --word-diff
  cargo test -p codex-tui
  ```


**DON’Ts**

- Don’t dump the entire CallToolResult struct as JSON: Show content items (text/image/audio/resource) instead.

- Don’t pretty-print multi-line JSON blocks: Ratatui wraps at whitespace; use single-line-with-spaces compact JSON.

- Don’t truncate by byte/char count or assume 1 grapheme == 1 cell: Use grapheme-aware truncation and budget per visible columns.

- Don’t silently drop unhandled content types: If images/audio aren’t rendered yet, show explicit placeholders and leave a TODO.

- Don’t misname layout parameters: Avoid terminal_width when you really mean num_cols.

- Don’t let helpers bloat widget files: Move formatting/truncation into a helper module with unit tests.

- Don’t reorder Cargo.toml carelessly: Keep dependencies alpha-sorted; add features (like preserve_order) deliberately.

- Don’t construct styles verbosely when Stylize suffices: Prefer "text".green() over manual Style building.

- Don’t forget clear error styling: Prefix with “Error: ” in bold red and include the message.

- Don’t skip format!-inlining: Avoid format!("{}", var); write format!("{var}") instead.