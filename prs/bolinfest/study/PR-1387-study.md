**DOs**
- **Use `is_some`/`is_none`:** Prefer direct Option checks when the value isn’t used.
```rust
// Do
if event.response.is_some() {
    let _ = tx_event.send(Ok(ResponseEvent::Created)).await;
}
```

- **Prefer unit variants:** Use unit enum variants when they carry no data.
```rust
// Do
#[derive(Debug)]
pub enum ResponseEvent {
    Created,
    // ...
}

match evt {
    ResponseEvent::Created => { /* ... */ }
    _ => {}
}
```

- **Borrow with `Cow` to avoid clones:** Only allocate when you must change the data.
```rust
use std::borrow::Cow;

fn with_missing_calls<'a>(prompt: &'a Prompt, missing_calls: Vec<ResponseItem>) -> Cow<'a, Prompt> {
    if missing_calls.is_empty() {
        Cow::Borrowed(prompt)
    } else {
        Cow::Owned(Prompt {
            input: [missing_calls, prompt.input.clone()].concat(),
            ..prompt.clone()
        })
    }
}

// Usage
let prompt = with_missing_calls(prompt, missing_calls);
let mut stream = sess.client.clone().stream(&prompt).await?;
```

- **Explain non-obvious transformations:** Add a concise comment where behavior isn’t obvious.
```rust
// The Responses API rejects follow-ups until all prior call_ids have outputs.
// If the previous turn was interrupted, prepend synthetic "aborted" outputs
// so this turn won’t 400 on missing call_ids.
let prompt = with_missing_calls(prompt, missing_calls);
```

- **Use struct update syntax:** Keep reconstructed structs concise and correct.
```rust
// Do
let prompt = Prompt {
    input: [missing_calls, prompt.input.clone()].concat(),
    ..prompt.clone()
};
```

**DON’Ts**
- **Don’t pattern-match just to test presence:** Avoid `if let Some(_) = ...` when you don’t use the value.
```rust
// Don't
if let Some(_) = event.response { /* ... */ }
```

- **Don’t define empty struct-like variants:** Avoid `Created {}` when no fields exist.
```rust
// Don't
pub enum ResponseEvent {
    Created {},
}
```

- **Don’t clone large structs unnecessarily:** Avoid `prompt.clone()` unless you’re actually modifying it.
```rust
// Don't
let prompt = prompt.clone(); // unnecessary if unchanged
```

- **Don’t rebuild every field manually:** Prefer `..prompt.clone()` over listing unchanged fields.
```rust
// Don't
let prompt = Prompt {
    input,
    prev_id: prompt.prev_id.clone(),
    user_instructions: prompt.user_instructions.clone(),
    store: prompt.store,
    extra_tools: prompt.extra_tools.clone(),
};
```

- **Don’t leave rationale implicit:** When rewriting inputs (e.g., injecting missing call outputs), document why to aid future maintenance.