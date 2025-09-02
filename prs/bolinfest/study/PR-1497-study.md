**DOs**

- **Align Enum And Match Order:** Keep `match` arms in the same logical order as variants in the enum; place new, less-used variants near the end.
```rust
// enum (place less-frequent variants later)
pub enum Op {
    ConfigureSession { /* ... */ },
    // ...
    EraseConversationHistory,
}

// match (mirror the enum’s structure)
match sub.op {
    Op::ConfigureSession { .. } => { /* ... */ }
    // ...
    Op::EraseConversationHistory => {
        if let Some(sess) = sess.as_ref() {
            sess.erase_conversation_history();
        }
    }
}
```

- **Keep Comments Tight:** Prefer brief comments that state intent; link out if needed.
```rust
// Fully reset server-side context, too.
state.previous_response_id = None;
```

- **Group Test Imports Clearly:** Put all `use` items for tests together at the top of `mod tests`.
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ContentItem, ResponseItem};

    #[test]
    fn clear_removes_all_items() { /* ... */ }
}
```

- **Centralize Model Calls In Core:** Expose a single core helper that respects the configured provider (Responses or Chat Completions), and call it from the TUI.
```rust
// core
pub async fn summarize_messages(
    cfg: &Config,
    model: &str,
    messages: Vec<Message>
) -> anyhow::Result<String> {
    let base = cfg.model_provider.base_url.trim_end_matches('/');
    let path = if cfg.model_provider.prefers_responses() { "/responses" } else { "/chat/completions" };
    let url = format!("{}{}", base, path);
    // ... perform request using shared Client and schema ...
    Ok(summary)
}

// tui
let summary = codex_core::client::summarize_messages(&self.config, &self.config.model, messages).await?;
```

- **Prefer Non-Blocking Async:** Use `tokio::spawn` for async I/O; avoid blocking the runtime.
```rust
let tx = app_event_tx.clone();
tokio::spawn(async move {
    let result = generate_compact_summary(&transcript, &model, &config).await;
    let evt = AppEvent::CompactComplete(result.map_err(|e| format!("{e}")));
    tx.send(evt);
});
```

- **Consider A Dedicated Op For Compaction:** Push compaction into core so TUI stays thin and purely event-driven.
```rust
// protocol
pub enum Op {
    ConfigureSession { /* ... */ },
    // ...
    CompactConversationHistory { transcript: Vec<Message> },
}

// tui
self.submit_op(Op::CompactConversationHistory { transcript });

// core
Op::CompactConversationHistory { transcript } => {
    let summary = summarize_messages(&cfg, &cfg.model, transcript).await?;
    sess.erase_conversation_history();
    // ... send summary back via existing event/stream path ...
}
```

- **Clear Frontend And Backend State:** When compacting, wipe UI history and backend identifiers.
```rust
self.conversation_history.clear_agent_history();
self.transcript.clear();
self.submit_op(Op::EraseConversationHistory); // clears transcript + previous_response_id
```

---

**DON’Ts**

- **Don’t Mismatch Enum/Match Placement:** Avoid placing the `match` arm for a variant far from where the variant appears in the enum.
```rust
// Don’t do this: variant is last, but arm appears first.
match sub.op {
    Op::EraseConversationHistory => { /* ... */ }, // misplaced
    Op::ConfigureSession { .. } => { /* ... */ },
}
```

- **Don’t Write Paragraph-Long Comments:** Avoid long narrative comments in code bodies.
```rust
// Don’t: multi-line essay about API behavior...
// Do: a one-liner plus a link if necessary.
```

- **Don’t Duplicate HTTP Logic In TUI:** TUI shouldn’t hardcode endpoints or JSON shapes; don’t assume Chat Completions is always available.
```rust
// Don’t: hardcoded path and bespoke JSON in TUI.
let url = format!("{}/chat/completions", base);
let body: serde_json::Value = client.post(url).json(&payload).send().await?.json().await?;
```

- **Don’t Block The Runtime For Async Work:** Avoid `spawn_blocking` + `rt.block_on` for network calls.
```rust
// Don’t
tokio::task::spawn_blocking(move || {
    tokio::runtime::Handle::current().block_on(async { /* network */ });
});
```

- **Don’t Only Clear Local UI State:** Erasing history must also drop server-side identifiers so past context can’t leak back in.
```rust
// Don’t: only clear UI entries.
// Do: also clear session’s server-side handle/id.
```

- **Don’t Scatter Test-Local Imports:** Avoid importing types inside test functions when they’re used across the module.
```rust
// Don’t
#[test]
fn t() {
    use crate::models::ContentItem; // scattered import
    // ...
}
```