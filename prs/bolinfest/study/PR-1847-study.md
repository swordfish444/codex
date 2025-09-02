**DOs**
- **Built‑in Provider ID**: Reference the OSS provider via the constant and `built_in_model_providers`.
```rust
let prov = codex_core::built_in_model_providers()
    .get(codex_core::BUILT_IN_OSS_MODEL_PROVIDER_ID)
    .expect("oss provider");
let base = prov.base_url.as_deref().expect("base_url");
```

- **Base URL Defaults**: Honor `CODEX_OSS_BASE_URL`, fall back to `CODEX_OSS_PORT` or `11434`, and suffix `/v1`.
```rust
let port = std::env::var("CODEX_OSS_PORT")
    .ok()
    .and_then(|v| v.parse::<u32>().ok())
    .unwrap_or(11434);
let base_url = std::env::var("CODEX_OSS_BASE_URL")
    .ok()
    .filter(|s| !s.trim().is_empty())
    .unwrap_or_else(|| format!("http://localhost:{port}/v1"));
```

- **Health Probe**: Detect OpenAI‑compatible roots (`.../v1`) and probe the right endpoint.
```rust
let root = codex_ollama::url::base_url_to_host_root(base_url);
let url = if codex_ollama::url::is_openai_compatible_base_url(base_url) {
    format!("{root}/v1/models")
} else {
    format!("{root}/api/tags")
};
let ok = reqwest::Client::new().get(url).send().await?.status().is_success();
```

- **Profile Macro**: Prefer `--profile oss` (backed by a profile) to pick provider and default model together.
```toml
# config.toml
[profile.oss]
model_provider = "oss"
default_model = "llama3.2:3b"
```
```rust
// Normalize to profile-based flow
let overrides = ConfigOverrides {
    config_profile: Some("oss".into()), // when user selects OSS shortcut
    model_provider: None,               // let profile supply this
    ..Default::default()
};
```

- **Exec Parity**: Mirror the TUI OSS shortcut/profile behavior in `codex exec`.
```rust
// In exec CLI bootstrap
if cli.config_profile.as_deref() == Some("oss") {
    let client = codex_ollama::OllamaClient::from_oss_provider();
    if !client.probe_server().await? {
        eprintln!("Ollama not reachable at {}.", client.get_host());
        std::process::exit(1);
    }
}
```

- **Streamed Pull**: Parse newline‑delimited JSON, emit events, and aggregate progress for a clean UX.
```rust
let mut stream = resp.bytes_stream();
let mut buf = bytes::BytesMut::new();
while let Some(chunk) = stream.next().await {
    buf.extend_from_slice(&chunk?);
    while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
        let line = buf.split_to(nl + 1);
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&line) {
            for ev in pull_events_from_value(&v) { /* handle PullEvent */ }
        }
    }
}
```

**DON'Ts**
- **Don’t Mutate User Config**: Do not write providers into `config.toml` at runtime; use built‑ins and overrides.
```rust
// Good: ephemeral override, no file writes
let overrides = ConfigOverrides {
    model_provider: Some("oss".into()),
    ..Default::default()
};
```

- **Don’t Require Auth For OSS**: Keep `requires_auth = false` and omit OpenAI‑specific headers for the OSS provider.
```rust
ModelProviderInfo {
    name: "Open Source".into(),
    base_url: Some(base_url),
    requires_auth: false,
    http_headers: None,
    env_http_headers: None,
    wire_api: WireApi::Chat,
    ..Default::default()
}
```

- **Don’t Hardcode Native Endpoints**: Avoid always calling `/api/tags`; switch to `/v1/models` when base URL ends with `/v1`.
```rust
let url = if is_openai_compatible_base_url(base_url) {
    format!("{root}/v1/models")
} else {
    format!("{root}/api/tags")
};
```

- **Don’t Spam Status Output**: Filter noisy statuses and render single‑line, in‑place progress updates.
```rust
if status.eq_ignore_ascii_case("pulling manifest") {
    // skip noisy manifest lines
} else {
    eprint!("\r{status}");
    std::io::stderr().flush()?;
}
```

- **Don’t Make TUI‑Only UX**: Any OSS‑selection shortcut must also exist in `codex exec`.
```rust
// Share the same normalization path
let profile = cli.config_profile.or_else(|| cli.oss.then(|| "oss".into()));
let overrides = ConfigOverrides { config_profile: profile, ..Default::default() };
```

- **Don’t Run Networked Tests In Sandbox**: Skip wiremock/network tests when sandboxed without network.
```rust
#[tokio::test]
async fn test_fetch_models() {
    if std::env::var(codex_core::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        tracing::info!("sandboxed; skipping");
        return;
    }
    // wiremock-based test body...
}
```