**DOs**

- **Centralize Provider Tuning**: Put retry/timeout fields on `ModelProviderInfo` for all providers, with clear defaults.
```rust
// model_provider_info.rs
const DEFAULT_REQUEST_MAX_RETRIES: u64 = 4;
const DEFAULT_STREAM_MAX_RETRIES: u64 = 10;
const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 300_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelProviderInfo {
    // ...
    pub request_max_retries: Option<u64>,
    pub stream_max_retries: Option<u64>,
    pub stream_idle_timeout_ms: Option<u64>,
}

impl ModelProviderInfo {
    pub fn request_max_retries(&self) -> u64 {
        self.request_max_retries.unwrap_or(DEFAULT_REQUEST_MAX_RETRIES)
    }
    pub fn stream_max_retries(&self) -> u64 {
        self.stream_max_retries.unwrap_or(DEFAULT_STREAM_MAX_RETRIES)
    }
    pub fn stream_idle_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(
            self.stream_idle_timeout_ms.unwrap_or(DEFAULT_STREAM_IDLE_TIMEOUT_MS),
        )
    }
}
```

- **Use Provider Values Everywhere**: Plumb provider tuning into retry loops and SSE processors.
```rust
// client.rs
let mut attempt = 0;
let max_retries = self.provider.request_max_retries();
loop {
    attempt += 1;
    match do_request().await {
        Ok(resp) if resp.status().is_success() => {
            let (tx, rx) = tokio::sync::mpsc::channel(1600);
            let stream = resp.bytes_stream().map_err(CodexErr::Reqwest);
            tokio::spawn(process_sse(stream, tx, self.provider.stream_idle_timeout()));
            return Ok(ResponseStream { rx_event: rx });
        }
        Ok(res) if attempt > max_retries => return Err(CodexErr::RetryLimit(res.status())),
        Ok(_) | Err(_) => tokio::time::sleep(backoff(attempt)).await,
    }
}
```

- **Treat Early Close as Error**: If the stream ends before `response.completed`, bubble an error to trigger retries.
```rust
// codex.rs
loop {
    let event = match stream.next().await {
        None => return Err(CodexErr::Stream("stream closed before response.completed".into())),
        Some(Err(e)) => return Err(e),
        Some(Ok(ev)) => ev,
    };
    // handle event variants ...
}
```

- **Document Under model_providers**: Keep tuning options in the `model_providers` section; use readable integer literals.
```toml
# config.toml
[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
request_max_retries = 4
stream_max_retries = 10
stream_idle_timeout_ms = 300_000
```

- **Configure Tests via Config**: Avoid process env mutation; inject provider settings directly in tests.
```rust
// tests
let provider = ModelProviderInfo {
    name: "test".into(),
    base_url: format!("{}/v1", server.uri()),
    env_key: Some("TEST_API_KEY".into()),
    wire_api: WireApi::Responses,
    request_max_retries: Some(0),
    stream_max_retries: Some(1),
    stream_idle_timeout_ms: Some(2_000),
    ..Default::default()
};
```

- **Keep Style Tight**: Inline variables in format strings, use meaningful names, remove dead code/comments, prefer US English.
```rust
// good
warn!("stream disconnected - retrying turn ({retries}/{max_retries} in {delay:?})...");

// good
let event = stream.next().await;

// remove unused args and unreachable tails
// fn process_sse(stream, tx, idle_timeout) { /* ... */ }
// (no trailing `Ok(output)` after exhaustive returns)
```


**DON’Ts**

- **Don’t Use OPENAI_* Env Flags**: Configure retry/timeouts via `Config`, not `OPENAI_REQUEST_MAX_RETRIES`, etc.
```rust
// bad
unsafe {
    std::env::set_var("OPENAI_STREAM_MAX_RETRIES", "2");
}
```

- **Don’t Prefix Keys With openai_**: Make tuning keys provider-agnostic.
```toml
# bad
openai_stream_idle_timeout_ms = 300000

# good
stream_idle_timeout_ms = 300_000
```

- **Don’t Hardcode Global Flags In SSE**: Pass idle timeout from the provider, not a static `OPENAI_STREAM_IDLE_TIMEOUT_MS`.
```rust
// bad
tokio::spawn(process_sse(stream, tx_event /* no timeout */));

// good
tokio::spawn(process_sse(stream, tx_event, provider.stream_idle_timeout()));
```

- **Don’t Swallow Early Stream Termination**: Avoid `while let Some(Ok(event))` that ignores `None` and `Err`.
```rust
// bad
while let Some(Ok(event)) = stream.next().await { /* ... */ }

// good
let ev = stream.next().await;
```

- **Don’t Leave Dead/Unformatted Bits**: Remove unused params/methods and unreachable returns; keep one clean format string.
```rust
// bad
warn!("retrying turn ({retries}/{} in {:?})...", max_retries, delay);

// good
warn!("retrying turn ({retries}/{max_retries} in {delay:?})...");
```

- **Don’t Reshuffle Docs Arbitrarily**: Don’t change heading levels or move tuning docs outside the `model_providers` section. Keep wording consistent (American English, straight quotes).