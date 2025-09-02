**DOs**
- **Centralize auth requirement in `ModelFamily`: add `requires_chatgpt_auth` flag.**
```rust
// model_family.rs
pub struct ModelFamily {
    pub family: String,
    pub supports_reasoning_summaries: bool,
    pub requires_chatgpt_auth: bool,
    // ...
}

// When registering a new family:
} else if slug.starts_with("2025-08-06-model") {
    model_family!(
        slug, "2025-08-06-model",
        supports_reasoning_summaries: true,
        requires_chatgpt_auth: true,
    )
}
```

- **Enforce auth generically in the client using the flag, not the slug.**
```rust
// client.rs
let auth_mode = auth.as_ref().map(|a| a.mode);
let mf = &self.config.model_family;

if mf.requires_chatgpt_auth && auth_mode != Some(AuthMode::ChatGPT) {
    return Err(CodexErr::UnexpectedStatus(
        StatusCode::BAD_REQUEST,
        format!(
            "{} is only supported with ChatGPT auth, run `codex login status` \
             to check your auth status and `codex login` to login with ChatGPT",
            mf.family
        ),
    ));
}
```

- **Register concrete model limits in `openai_model_info.rs`.**
```rust
// openai_model_info.rs
match model_family.family.as_str() {
    "2025-08-06-model" => Some(ModelInfo {
        context_window: 200_000,
        max_output_tokens: 100_000,
    }),
    _ => None,
}
```

- **Keep capabilities single-sourced via `ModelFamily` flags.**
```rust
// Example: gating a feature on a capability, not on a slug.
let mf = &self.config.model_family;
if mf.supports_reasoning_summaries {
    // produce reasoning summaries
}
```

**DON’Ts**
- **Don’t hardcode slug checks in client logic.**
```rust
// Anti-pattern: brittle and scatters policy across code.
if self.config.model_family.family == "2025-08-06-model"
    && auth_mode != Some(AuthMode::ChatGPT)
{
    /* ... */
}
```

- **Don’t add a new family without declaring all relevant flags.**
```rust
// Anti-pattern: missing `requires_chatgpt_auth` leads to silent policy gaps.
model_family!(
    slug, "2025-08-06-model",
    supports_reasoning_summaries: true
    // requires_chatgpt_auth: true // <-- forgotten
);
```

- **Don’t duplicate or vary the user guidance string; format it once with the family.**
```rust
// Anti-pattern: copy-pasted, model-specific message string.
return Err(CodexErr::UnexpectedStatus(
    StatusCode::BAD_REQUEST,
    "2025-08-06-model is only supported with ChatGPT auth...".to_string(),
));

// Prefer the generic, parameterized message shown in the DOs section.
```