**PR #1527 Review Takeaways (bolinfest)**

**DOs**
- Bold prompts: keep multi-line or escape-heavy text as raw strings.
  ```rust
  const PROMPT: &str = r#"Please summarize the conversation so far:
  - Key points
  - Decisions
  - Next steps
  "#;
  ```

- Externalize long prompts in Markdown and `include_str!()` them.
  ```md
  // SUMMARY.md
  You are a summarization assistant. Produce:
  - Objective
  - User instructions
  - AI actions
  - Important entities
  - Open issues / next steps
  - Concise summary
  ```
  ```rust
  const SUMMARY_INSTRUCTIONS: &str = include_str!("../../../SUMMARY.md");
  ```

- Clone `Arc` values idiomatically with `.clone()` when you own the `Arc`.
  ```rust
  // Better (concise and conventional)
  let task = AgentTask::spawn(sess.clone(), sub_id, items);

  // Also OK when you only have &Arc<T>:
  let task = AgentTask::spawn(Arc::clone(&sess), sub_id, items);
  ```

- Centralize “turn input” assembly behind a helper to avoid duplication.
  ```rust
  // Helper on Session:
  pub fn turn_input_with_history(&self, extra: Vec<ResponseItem>) -> Vec<ResponseItem> {
      [self.state.lock().unwrap().history.contents(), extra].concat()
  }

  // Usage:
  let turn_input = sess.turn_input_with_history(pending_input);
  ```

- Use captured identifiers in `format!` for clarity and brevity.
  ```rust
  sess.notify_background_event(
      &sub_id,
      format!("stream error: {e}; retrying {retries}/{max_retries} in {delay:?}…"),
  ).await;
  ```

- Prefer positive, specific assertions in tests (assert what should happen).
  ```rust
  // Verify summarization instructions applied on the second request
  let instr1 = body1["instructions"].as_str().unwrap();
  let instr2 = body2["instructions"].as_str().unwrap();
  assert_ne!(instr1, instr2);
  assert!(instr2.contains("You are a summarization assistant"));

  // Verify compaction effect on third request
  let assistant_count = messages.iter().filter(|(r, _)| r == "assistant").count();
  assert_eq!(assistant_count, 1);
  assert!(messages.iter().any(|(r,t)| r == "user" && t == THIRD_USER_MSG));
  assert!(!messages.iter().any(|(_,t)| t.contains("hello world")));
  ```

- Use doc comments for test descriptions; keep comments minimal and purposeful.
  ```rust
  /// When there's no current task, SummarizeContext spawns a new AgentTask.
  #[tokio::test]
  async fn summarize_spawns_when_idle() { /* ... */ }
  ```

- Where available, use test wrappers/utilities that wait for “configured” state instead of hand-rolled loops.


**DON’Ts**
- Don’t build long strings with `concat!()` for readability-sensitive prompts.
  ```rust
  // Avoid
  const PROMPT: &str = concat!(
      "Please provide a summary of our conversation so far, ",
      "highlighting key points and decisions."
  );
  ```

- Don’t inline large instruction text directly in code when it belongs in a file.
  ```rust
  // Avoid giant multi-line literals in source; prefer SUMMARY.md + include_str!()
  ```

- Don’t use `Arc::clone(sess)` without a reference; it’s easy to get wrong and is noisier than `.clone()`.
  ```rust
  // Avoid
  let task = AgentTask::spawn(Arc::clone(sess), sub_id, items); // missing &
  ```

- Don’t manually stitch history everywhere when a helper exists.
  ```rust
  // Avoid
  let turn_input = [sess.state.lock().unwrap().history.contents(), pending_input].concat();
  ```

- Don’t write tests that pass by the absence of events or timeouts; assert on the expected event/data instead.
  ```rust
  // Avoid: asserting that “no TaskStarted arrived”
  let result = timeout(Duration::from_millis(500), codex.next_event()).await;
  assert!(result.is_err());

  // Prefer: assert you received the expected event/message
  let ev = wait_for_event(&codex, |e| matches!(e.msg, EventMsg::TaskComplete(_))).await;
  ```

- Don’t repeat in comments what your `assert!` already states; keep tests self-explanatory via assertions.
  ```rust
  // Avoid redundant comments; write a clear assertion message instead
  assert!(task_started, "Expected TaskStarted when no current task exists");
  ```