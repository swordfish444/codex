**DOs**
- **Override semantics**: Use `insert` so user config replaces built-ins.
```rust
let mut model_providers = built_in_model_providers();
for (key, provider) in cfg.model_providers.into_iter() {
    model_providers.insert(key, provider); // user-defined wins
}
```
- **Add focused test**: Assert that a user-defined provider with the same key overrides the built-in.
```rust
#[test]
fn user_provider_overrides_builtin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut providers = std::collections::HashMap::new();
    providers.insert("oss".to_string(), ModelProviderInfo {
        name: "Custom".into(),
        base_url: Some("https://example.com/v1".into()),
        env_key: None,
        env_key_instructions: None,
        wire_api: WireApi::Chat,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        requires_openai_auth: false,
    });
    let cfg = ConfigToml {
        model_provider: Some("oss".to_string()),
        model: Some("gpt-oss:20b".to_string()),
        model_providers: providers,
        ..Default::default()
    };
    let config = Config::load_from_base_config_with_overrides(
        cfg, ConfigOverrides::default(), tmp.path().to_path_buf()
    ).unwrap();
    assert_eq!(config.model_provider.name, "Custom");
    assert_eq!(config.model_provider.base_url.as_deref(), Some("https://example.com/v1"));
}
```
- **Config override**: Document how to override in `config.toml`.
```toml
model_provider = "oss"
model = "gpt-oss:20b"

[model_providers.oss]
name = "Custom"
base_url = "https://example.com/v1"
requires_openai_auth = false
```
- **Accurate comments**: Keep comments precise and typo-free (apply review suggestions).
```rust
// Override the built-in provider if the same key is present in config.toml
```

**DON'Ts**
- **Use `or_insert` for overrides**: It prevents user config from replacing built-ins.
```rust
for (key, provider) in cfg.model_providers.into_iter() {
    model_providers.entry(key).or_insert(provider); // DON'T: built-in stays
}
```
- **Ship typos**: Avoid unclear or misspelled comments.
```rust
// Override ... if the same key is present ib config.toml // DON'T: typo "ib"
```
- **Omit tests for behavior change**: Don’t rely on comments alone—verify override behavior.
```rust
// DON'T: Missing test for user-overrides-builtin semantics
// Add an assertion-based test like `user_provider_overrides_builtin`.
```
- **Define incomplete providers**: Don’t omit required fields for the overridden provider.
```toml
[model_providers.oss]
name = "Custom"
# base_url is missing — DON'T if the provider requires it
```