**DOs**
- Use serde rename for reserved keys: map JSON "type" to a Rust-friendly field to avoid raw identifiers.
```rust
#[derive(Debug, Deserialize)]
struct Error {
    #[serde(rename = "type")]
    error_type: Option<String>,
    message: Option<String>,
    plan_type: Option<String>,
    resets_in_seconds: Option<u64>,
}
```

- Prefer structured fields over string parsing: use server-provided fields for logic and user messaging.
```rust
if error.error_type.as_deref() == Some("usage_limit_reached") {
    let plan_type = error.plan_type.or_else(|| auth.and_then(|a| a.get_plan_type()));
    let resets_in_seconds = error.resets_in_seconds;
    return Err(CodexErr::UsageLimitReached(UsageLimitReachedError {
        plan_type,
        resets_in_seconds,
    }));
}
```

- Keep a safe fallback for other/unknown error types: don’t assume only one error type; degrade gracefully.
```rust
match error.error_type.as_deref() {
    Some("usage_not_included") => Err(CodexErr::UsageNotIncluded),
    Some("usage_limit_reached") => { /* handled above */ unreachable!() }
    _ => {
        let msg = error.message.unwrap_or_default();
        Err(CodexErr::Stream(msg, None))
    }
}
```

- Cover variants with tests: assert formatting and behavior across plans and reset timings.
```rust
#[test]
fn formats_minutes() {
    let e = UsageLimitReachedError { plan_type: None, resets_in_seconds: Some(5 * 60) };
    assert_eq!(e.to_string(), "You've hit your usage limit. Try again in 5 minutes.");
}

#[test]
fn formats_plus_with_hours_minutes() {
    let e = UsageLimitReachedError { plan_type: Some("plus".into()), resets_in_seconds: Some(3*3600 + 32*60) };
    assert_eq!(
        e.to_string(),
        "You've hit your usage limit. Upgrade to Pro (https://openai.com/chatgpt/pricing) or try again in 3 hours 32 minutes."
    );
}
```

**DON’Ts**
- Don’t use raw identifiers for reserved JSON keys: avoid `r#type` in structs.
```rust
// Avoid
#[derive(Debug, Deserialize)]
struct Error {
    r#type: Option<String>, // brittle and noisy to use
}
```

- Don’t parse delays from error message strings: drop regex-based “Please try again in …” parsing in favor of typed fields.
```rust
// Avoid
let re = Regex::new(r"Please try again in (\d+(\.\d+)?)(s|ms)").unwrap();
// Fragile: message formats change; prefer `resets_in_seconds`.
```

- Don’t remove fallback handling for other error types: ensure non–rate-limit errors still map to sensible `CodexErr`s.
```rust
// Avoid: handling only one type and dropping the rest
if error.error_type.as_deref() == Some("usage_limit_reached") {
    /* ... */
} // Missing else branches loses information for other errors
```