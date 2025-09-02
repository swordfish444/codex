**DOs**
- **Alphabetize dev-dependencies:** Keep `[dev-dependencies]` keys in `Cargo.toml` strictly alpha-sorted.
  ```toml
  [dev-dependencies]
  predicates = "3"
  pretty_assertions = "1.4.1"
  tempfile = "3"
  tokio-test = "0.4"
  wiremock = "0.6"
  ```

- **Assert sequences in one go:** Prefer a single equality check over many piecemeal asserts by normalizing to a comparable representation.
  ```rust
  // #[tokio::test]
  // async fn example() -> Result<(), CodexErr> {
  let got: Vec<Result<ResponseEvent>> = collect_events(&chunks).await;

  // Normalize to comparable labels, then assert once.
  let kinds: Vec<_> = got
      .into_iter()
      .map(|r| r.map(|e| match e {
          ResponseEvent::OutputItemDone(_) => "output_item.done",
          ResponseEvent::Completed { .. } => "completed",
          ResponseEvent::Created => "created",
          _ => "other",
      }))
      .collect::<Result<Vec<_>, _>>()?; // Vec<Result<T>> -> Result<Vec<T>>

  assert_eq!(kinds, vec!["output_item.done", "output_item.done", "completed"]);
  // Ok(())
  // }
  ```

- **Convert Vec<Result<T>> with collect:** Collapse `Vec<Result<T>>` into `Result<Vec<T>>` before asserting to simplify code.
  ```rust
  let raw: Vec<Result<ResponseEvent>> = collect_events(&chunks).await;
  let events: Vec<ResponseEvent> = raw.into_iter().collect::<Result<_, _>>()?;
  // Now you can assert on `events` in one shot (possibly after mapping).
  ```

- **Interleave fixtures with usage:** Define each JSON fixture next to its corresponding SSE string to make the flow obvious.
  ```rust
  let item1 = json!({ "type": "response.output_item.done", "item": { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "Hello" }] } }).to_string();
  let sse1  = format!("event: response.output_item.done\ndata: {item1}\n\n");

  let item2 = json!({ "type": "response.output_item.done", "item": { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "World" }] } }).to_string();
  let sse2  = format!("event: response.output_item.done\ndata: {item2}\n\n");

  let completed = json!({ "type": "response.completed", "response": { "id": "resp1" } }).to_string();
  let sse3      = format!("event: response.completed\ndata: {completed}\n\n");

  let events = collect_events(&[sse1.as_bytes(), sse2.as_bytes(), sse3.as_bytes()]).await;
  ```

- **Use format! inline variables:** When constructing SSE lines, inline variables directly in the format string.
  ```rust
  let sse = format!("event: {event}\ndata: {payload}\n\n");
  ```

**DON’Ts**
- **Don’t leave deps unsorted:** Avoid appending new dev-dependencies at the end out of order.
  ```toml
  # Bad: not alpha-sorted — `tokio-test` tacked on at the end.
  [dev-dependencies]
  predicates = "3"
  pretty_assertions = "1.4.1"
  tempfile = "3"
  wiremock = "0.6"
  tokio-test = "0.4"
  ```

- **Don’t use many piecemeal asserts:** Avoid multiple element-by-element checks when a single assertion over the full sequence suffices.
  ```rust
  // Bad: fragile and noisy
  assert!(matches!(events[0], Ok(ResponseEvent::OutputItemDone(_))));
  assert!(matches!(events[1], Ok(ResponseEvent::OutputItemDone(_))));
  match &events[2] {
      Ok(ResponseEvent::Completed { .. }) => {}
      _ => panic!("unexpected"),
  }
  ```

- **Don’t separate related variables:** Avoid defining all `item*` first and all `sse*` later; it obscures the intended order.
  ```rust
  // Bad: related data far apart; harder to reason about test flow.
  let item1 = /* ... */;
  let item2 = /* ... */;
  let item3 = /* ... */;

  let sse1 = format!("event: ...\ndata: {item1}\n\n");
  let sse2 = format!("event: ...\ndata: {item2}\n\n");
  let sse3 = format!("event: ...\ndata: {item3}\n\n");
  ```

- **Don’t overcomplicate result handling:** Avoid manual loops to peel `Result` values when `collect()` expresses the intent succinctly.
  ```rust
  // Bad: verbose manual extraction
  let mut ok = Vec::new();
  for e in raw {
      ok.push(e?);
  }
  // Prefer: raw.into_iter().collect::<Result<Vec<_>, _>>()?
  ```