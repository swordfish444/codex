**DOs**
- **Prefer provider env key**: Use the provider’s `env_key` API key over any default Codex auth when both are present.
```rust
// Inside ModelProviderInfo::create_request_builder
let effective_auth = match self.api_key() {
    Ok(Some(key)) => Some(CodexAuth::from_api_key(key)),
    Ok(None) => auth.clone(),
    Err(err) => {
        if auth.is_some() { auth.clone() } else { return Err(err); }
    }
};

let mut builder = client.post(self.get_full_url(&effective_auth));
if let Some(a) = effective_auth.as_ref() {
    builder = builder.bearer_auth(a.get_token().await?);
}
```

- **Use requires_openai_auth**: Replace uses of `requires_auth` with `requires_openai_auth` and treat it as “needs OpenAI auth (ChatGPT token)” only.
```rust
let provider = ModelProviderInfo {
    name: "custom".into(),
    env_key: Some("MY_CUSTOM_API_KEY".into()),
    requires_openai_auth: false, // not OpenAI auth; uses provider API key
    ..Default::default()
};
```

- **Keep docstrings accurate**: Update comments to reflect the new auth precedence and field semantics.
```rust
/// Builds a POST request for this provider:
/// - Applies static + env-based headers.
/// - Uses API key from `env_key` when available; otherwise uses supplied `auth`.
/// - If `env_key` is configured but missing and no `auth` is supplied, returns Err.
```

- **Test the override behavior**: Verify the Authorization header comes from the provider env var, not the default auth.
```rust
Mock::given(method("POST"))
    .and(path("/openai/responses"))
    .and(header_regex(
        "Authorization",
        format!("Bearer {}", std::env::var("USER").unwrap()).as_str(),
    ))
    .respond_with(ResponseTemplate::new(200))
    .expect(1)
    .mount(&server)
    .await;
```

- **Gate login UI on OpenAI auth only**: Show the login screen iff `requires_openai_auth` is true.
```rust
fn should_show_login_screen(config: &Config) -> bool {
    if config.model_provider.requires_openai_auth {
        // potentially perform OpenAI token checks
        return true;
    }
    false
}
```

- **Use inline format substitutions**: Keep string building concise.
```rust
let auth_header = format!("Bearer {}", token);
```

- **Gracefully degrade when env var missing**: Use provided `auth` if available; otherwise error.
```rust
let key = self.api_key(); // Err if env_key configured but missing
let effective = match key {
    Ok(Some(k)) => Some(CodexAuth::from_api_key(k)),
    Ok(None) => auth.clone(),
    Err(e) if auth.is_some() => auth.clone(),
    Err(e) => return Err(e),
};
```

**DON’Ts**
- **Don’t conflate auth types**: Do not treat `requires_openai_auth` as “any auth”. It indicates OpenAI-specific auth (ChatGPT token), not provider API keys.
```rust
// Wrong: requiring OpenAI auth for a provider that only needs its own API key
let provider = ModelProviderInfo {
    env_key: Some("MY_CUSTOM_API_KEY".into()),
    requires_openai_auth: true, // ❌ incorrect
    ..Default::default()
};
```

- **Don’t rely on default auth when env key exists**: If `env_key` yields a key, it must take precedence over the passed Codex auth.
```rust
// ❌ Anti-pattern: ignoring env_key-derived key
let effective_auth = auth.clone(); // should prefer self.api_key() if present
```

- **Don’t fail early if a fallback exists**: If `env_key` is configured but missing, do not error out when a usable `auth` is provided.
```rust
// ❌ Anti-pattern: unconditional error
if self.api_key().is_err() {
    return Err(...); // should check if auth.is_some() first
}
```

- **Don’t leave stale docs or names**: Remove references to `requires_auth` and “require_api_key” behavior in comments or parameter names that no longer exist.
```rust
// ❌ Stale
/// When `require_api_key` is true ...
// ✅ Update to reflect env_key precedence and requires_openai_auth semantics.
```

- **Don’t reintroduce removed indirection**: Avoid resurrecting `get_fallback_auth`/`Cow<Option<CodexAuth>>` patterns—auth selection is now direct and explicit.
```rust
// ❌ Old pattern
let auth: Cow<'_, Option<CodexAuth>> = Cow::Owned(self.get_fallback_auth()?);
```