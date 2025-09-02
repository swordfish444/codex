**Streaming Markdown PR #1920 — Reviewer Takeaways (bolinfest)**

**DOs**
- Boldly separate unrelated changes: land independent fixes in their own commits/PRs.
  - Why: Keeps review focused and reduces risk. Example: handle a new SSE event in core separately from UI streaming changes.
  - Example:
    ```rust
    // Commit A (core): precise SSE handling
    match event_name.as_str() {
        "response.reasoning_summary_part.added" => { /* intentionally ignored */ }
        "response.reasoning_summary_text.done" => {
            let event = ResponseEvent::ReasoningSummaryDelta("\n\n".to_string());
            if tx_event.send(Ok(event)).await.is_err() { return; }
        }
        other => debug!(other, "sse event"),
    }

    // Commit B (tui): markdown streaming mechanics
    // ... all UI-only code here ...
    ```

- Be precise about where logic belongs: put new behavior on the exact event arm intended.
  - Why: Avoids accidental logging/no-op paths and makes intent obvious.
  - Example:
    ```rust
    match event_name.as_str() {
        // Correct: “part.added” is handled (ignored) distinctly,
        // so it doesn’t fall into `other` logging.
        "response.reasoning_summary_part.added" => { /* ignore */ }

        // Correct: the “text.done” terminator emits a separator.
        "response.reasoning_summary_text.done" => {
            let e = ResponseEvent::ReasoningSummaryDelta("\n\n".to_string());
            if tx_event.send(Ok(e)).await.is_err() { return; }
        }

        other => debug!(other, "sse event"),
    }
    ```

- Implement fenced code parsing per Markdown rules: detect fences of length ≥ 3 and require an exact-length match to close; support both backticks and tildes.
  - Why: Real markdown allows longer fences; closing fence length must equal opening length.
  - Example:
    ```rust
    fn fence_open(line: &str) -> Option<(&'static str, usize, Option<String>)> {
        let t = line.trim_start();
        for token in ["`", "~"] {
            let count = t.chars().take_while(|&c| c.to_string() == token).count();
            if count >= 3 {
                let rest = &t[count..];
                let lang = rest.trim().split_whitespace().next()
                    .filter(|s| !s.is_empty()).map(|s| s.to_string());
                return Some((token, count, lang));
            }
        }
        None
    }

    fn fence_close(line: &str, token: &str, len: usize) -> bool {
        let t = line.trim();
        t.chars().take_while(|&c| c.to_string() == token).count() == len
            && t.chars().skip(len).all(char::is_whitespace)
    }
    ```

- Guard streaming against partial fences: buffer inside an open fence and emit only after the closing fence arrives.
  - Why: Prevents stray backticks and malformed code in history.
  - Example:
    ```rust
    if in_fence {
        if fence_close(delta_line, fence_token, fence_len) {
            in_fence = false;
            commit(render_code_block(&buffered_code));
            buffered_code.clear();
        } else {
            buffered_code.push_str(delta_line);
        }
        continue;
    }
    ```

- Add tight tests for edge cases the spec permits: longer fences, mismatched closers, mixed tildes/backticks, and headings or lists adjacent to code blocks.
  - Example:
    ```rust
    // open with 5 backticks, close with exactly 5
    assert!(fence_open("`````rust").is_some());
    assert!(fence_close("`````", "`", 5));
    assert!(!fence_close("```", "`", 5)); // wrong length
    // support tildes too
    assert!(fence_open("~~~~~").is_some());
    ```

- Keep comments crisp and intent-focused: explain “why” (spec/contract) where non-obvious, not “what”.
  - Example:
    ```rust
    // Markdown: closing fence must equal opening fence length (CommonMark).
    if fence_close(line, token, len) { /* ... */ }
    ```

**DON’Ts**
- Don’t couple orthogonal fixes to UI refactors.
  - Anti-pattern:
    ```rust
    // Mixed in the streaming PR:
    // - add new SSE event handling in core
    // - change TUI streaming, animation, and tests
    // Reviewers can’t isolate the behavioral change from UI changes.
    ```
  - Preferred: land the SSE event handling first (or separately), then the TUI work.

- Don’t place new logic on the wrong match arm (or hide it behind “other” logging).
  - Anti-pattern:
    ```rust
    match event_name.as_str() {
        // Oops: both events lumped into the ignored set
        "response.reasoning_summary_part.added"
        | "response.reasoning_summary_text.done" => { /* ignored */ }
        other => debug!(other, "sse event"),
    }
    ```
  - Fix:
    ```rust
    "response.reasoning_summary_part.added" => { /* ignore */ }
    "response.reasoning_summary_text.done" => { /* emit separator */ }
    ```

- Don’t assume fences are exactly three backticks; don’t allow mismatched closers.
  - Anti-pattern:
    ```rust
    if line.trim_start().starts_with("```") { in_fence = true; }
    // ...
    if line.trim() == "```" { in_fence = false; } // wrong: ignores 4+, mismatch
    ```
  - Fix:
    ```rust
    if let Some((tok, len, _)) = fence_open(line) {
        fence_token = tok.to_string();
        fence_len = len;
        in_fence = true;
    }
    if fence_close(line, &fence_token, fence_len) { in_fence = false; }
    ```

- Don’t stream incomplete fenced content or fence markers into history.
  - Anti-pattern:
    ```rust
    history.push(format!("```{}", lang).into()); // shows the opening fence
    history.push(current_partial_code_line.into()); // before fence closes
    ```
  - Fix:
    ```rust
    if !in_fence { history.extend(committed_lines); }
    // When fence closes:
    history.extend(render_code_block_exact(&buffered_code));
    ```

- Don’t rely on default logging to signal correctness for ignored events.
  - Anti-pattern:
    ```rust
    // Let it fall into `other => debug!`
    ```
  - Fix:
    ```rust
    "response.reasoning_summary_part.added" => { /* intentionally ignored */ }
    // Ensures no misleading warnings/logging elsewhere.
    ```

- Don’t leave tests ambiguous about intent (e.g., “works” without spec reference).
  - Anti-pattern:
    ```rust
    #[test]
    fn fence_parsing_works() { /* vague, brittle */ }
    ```
  - Fix:
    ```rust
    #[test]
    fn closes_only_with_equal_length_fence() {
        let (tok, n, _) = fence_open("````txt").unwrap();
        assert!(fence_close("````", tok, n));
        assert!(!fence_close("```", tok, n));
    }
    ```