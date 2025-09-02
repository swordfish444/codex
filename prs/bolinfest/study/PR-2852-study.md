**DOs**

- **Emit Begin/End Pair:** Fire `WebSearchBegin` as a marker and include the actual `query` in `WebSearchEnd` for UI display.
  ```rust
  // Emission
  sess.tx_event.send(Event {
      id: sub_id.to_string(),
      msg: EventMsg::WebSearchBegin(WebSearchBeginEvent { call_id: call_id.clone() }),
  }).await.ok();

  sess.tx_event.send(Event {
      id: sub_id.to_string(),
      msg: EventMsg::WebSearchEnd(WebSearchEndEvent { call_id, query }),
  }).await.ok();
  ```

- **Validate IDs Strongly:** Require a non-empty `call_id` on `WebSearchCall` before emitting `WebSearchEnd`; log and skip otherwise.
  ```rust
  match item {
      ResponseItem::WebSearchCall {
          id: Some(call_id),
          action: WebSearchAction::Search { query },
          ..
      } if !call_id.is_empty() => {
          sess.tx_event.send(Event {
              id: sub_id.to_string(),
              msg: EventMsg::WebSearchEnd(WebSearchEndEvent { call_id, query }),
          }).await.ok();
      }
      ResponseItem::WebSearchCall { .. } => {
          warn!("web_search_call missing call_id; skipping end event");
      }
      _ => {}
  }
  ```

- **Centralize Tool Mapping:** Keep the `web_search` vs `web_search_preview` mapping in `OpenAiTool` (serde rename), and let `create_tools_json_for_responses_api` only serialize.
  ```rust
  #[derive(Serialize)]
  #[serde(tag = "type", rename_all = "snake_case")]
  enum OpenAiTool {
      #[serde(rename = "web_search_preview")]
      WebSearch {},
  }

  pub fn create_tools_json_for_responses_api(
      tools: &[OpenAiTool],
  ) -> anyhow::Result<Vec<serde_json::Value>> {
      tools.iter().map(serde_json::to_value).collect::<Result<_, _>>().map_err(Into::into)
  }

  // Client
  let tools_json = create_tools_json_for_responses_api(&prompt.tools)?;
  ```

- **Keep UI-Sensitive Fields Stable (or Plan Deprecations):** If removing `query` from `WebSearchBegin`, ensure UI is updated to read it from `WebSearchEnd` and communicate the change.

- **Exclude From History/Rollouts:** Treat `WebSearchCall` as non-history, non-rollout.
  ```rust
  fn is_api_message(m: &ResponseItem) -> bool {
      match m {
          ResponseItem::WebSearchCall { .. } | ResponseItem::Other => false,
          _ => true,
      }
  }
  ```

- **Simplify Ownership:** Prefer moves and defaults over unnecessary clones in config.
  ```rust
  let history = cfg.history.unwrap_or_default();
  let shell_environment_policy = cfg.shell_environment_policy.into();
  let tui = cfg.tui.unwrap_or_default();
  let chatgpt_base_url = config_profile.chatgpt_base_url
      .or(cfg.chatgpt_base_url)
      .unwrap_or("https://chatgpt.com/backend-api/".to_string());
  ```

- **Route SSE Explicitly:** Handle `"response.output_item.added"` separately to detect `"web_search_call"`; avoid lumping with unrelated kinds.
  ```rust
  match event.kind.as_str() {
      "response.output_item.added" => {
          if let Some(item) = event.item.as_ref() {
              if item.get("type").and_then(|v| v.as_str()) == Some("web_search_call") {
                  let call_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                  tx_event.send(Ok(ResponseEvent::WebSearchCallBegin { call_id })).await.ok();
              }
          }
      }
      "response.output_text.done" | "response.in_progress" => {}
      _ => {}
  }
  ```

- **Use Inline `format!` Captures:** Follow repo style for string formatting.
  ```rust
  // DO
  let msg = format!("Searched: {query}");
  ```

- **Update TUI on End:** Flush stream on begin; append a “Searched: …” history cell on end.
  ```rust
  fn on_web_search_begin(&mut self, _: WebSearchBeginEvent) {
      self.flush_answer_stream_with_separator();
  }
  fn on_web_search_end(&mut self, ev: WebSearchEndEvent) {
      let query = ev.query;
      self.add_to_history(history_cell::new_web_search_call(format!("Searched: {query}")));
  }
  ```


**DON’Ts**

- **Don’t Default Missing IDs:** Avoid `let call_id = id.unwrap_or_else(|| "".to_string());` which masks protocol issues.
  ```rust
  // DON'T
  let call_id = id.unwrap_or_else(|| "".to_string());
  // DO
  if let Some(call_id) = id.filter(|s| !s.is_empty()) { /* ... */ }
  ```

- **Don’t Show Query on Begin:** The begin event may precede the finalized query; display it on `WebSearchEnd` only.

- **Don’t Duplicate Tool Logic:** Don’t rewrite the tool `type` in multiple layers (e.g., client). Keep it in `OpenAiTool`’s serde mapping.

- **Don’t Persist WebSearchCall:** Don’t add `WebSearchCall` items to conversation history or rollout logs.

- **Don’t Over-Clone Config:** Avoid `.clone()` on `Option`/`Copy`-like config fields when `unwrap_or_default`, `.or(...)`, and `.into()` suffice.

- **Don’t Use Unsupported Tool Names:** Until the API accepts it reliably, don’t send `"web_search"`; use `"web_search_preview"` via serde rename.

- **Don’t Collapse SSE Branches:** Don’t combine `"response.output_item.added"` with other event kinds; precise routing prevents regressions.

- **Don’t Violate `format!` Style:** Avoid positional formatting when implicit captures are possible.
  ```rust
  // DON'T
  let msg = format!("Searched: {}", query);
  // DO
  let msg = format!("Searched: {query}");
  ```