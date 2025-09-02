**DOs**

- Include `session_id` header: Add the current session UUID to every Responses API request alongside required headers.
```rust
let rb = self.provider
    .create_request_builder(&self.client)?
    .header("OpenAI-Beta", "responses=experimental")
    .header("session_id", self.session_id.to_string())
    .header(reqwest::header::ACCEPT, "text/event-stream")
    .json(&payload);
```

- Thread `session_id` through client: Store it on the model client and pass it in at construction.
```rust
use uuid::Uuid;

pub struct ModelClient {
    // ...
    session_id: Uuid,
    // ...
}

impl ModelClient {
    pub fn new(/* ..., */ session_id: Uuid) -> Self {
        Self { /* ..., */ session_id, /* ... */ }
    }
}

// e.g., in submission loop
let client = ModelClient::new(/* ..., */ session_id);
```

- Preserve provider headers: Keep custom headers like `originator` when adding new ones.
```rust
let provider = ModelProviderInfo {
    base_url: format!("{}/v1", server.uri()),
    env_key: Some("PATH".into()),
    wire_api: codex_core::WireApi::Responses,
    http_headers: Some(
        [("originator".to_string(), "codex_cli_rs".to_string())]
            .into_iter()
            .collect(),
    ),
    ..provider
};
```

- Test with a mock SSE server: Use WireMock + fixtures; disable retries; skip when sandbox network is disabled.
```rust
if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
    println!("Skipping under Codex sandbox with network disabled.");
    return;
}

let server = MockServer::start().await;
let sse = load_sse_fixture_with_id("tests/fixtures/completed_template.json", "resp1");
Mock::given(method("POST"))
    .and(path("/v1/responses"))
    .respond_with(
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_raw(sse, "text/event-stream"),
    )
    .mount(&server)
    .await;

unsafe {
    std::env::set_var("OPENAI_REQUEST_MAX_RETRIES", "0");
    std::env::set_var("OPENAI_STREAM_MAX_RETRIES", "0");
}
```

- Assert both session and provider headers: Drive a tiny session, capture the session ID from events, and verify headers on the recorded request.
```rust
let (codex, _) = Codex::spawn(config, ctrl_c.clone()).await.unwrap();
codex.submit(Op::UserInput { items: vec![InputItem::Text { text: "hello".into() }] })
    .await.unwrap();

let mut sid = None;
loop {
    let ev = tokio::time::timeout(Duration::from_secs(1), codex.next_event())
        .await.unwrap().unwrap();
    if let EventMsg::SessionConfigured(SessionConfiguredEvent { session_id, .. }) = ev.msg {
        sid = Some(session_id.to_string());
    }
    if matches!(ev.msg, EventMsg::TaskComplete(_)) { break; }
}

let req = &server.received_requests().await.unwrap()[0];
assert_eq!(req.headers.get("session_id").unwrap().to_str().unwrap(), sid.as_deref().unwrap());
assert_eq!(req.headers.get("originator").unwrap().to_str().unwrap(), "codex_cli_rs");
```

- Use stable env for provider auth checks: Point `env_key` to a known variable like `PATH` to satisfy provider requirements in tests.
```rust
let provider = ModelProviderInfo { env_key: Some("PATH".into()), ..provider };
```

**DON’Ts**

- Don’t drop existing headers: Avoid rebuilding requests in a way that discards provider-configured headers.
```rust
// WRONG: loses provider headers like "originator"
let rb = reqwest::Client::new().post(url)
    .header("session_id", self.session_id.to_string());
```

- Don’t include `previous_response_id` on the first request: The initial Responses call must not carry it.
```rust
// WRONG: first Responses request must not set this
payload.previous_response_id = Some("resp0".into());
```

- Don’t omit SSE headers: The Accept and beta headers are required for streaming Responses.
```rust
// WRONG: missing required headers for SSE/Responses
let rb = self.provider.create_request_builder(&self.client)?
    .header("session_id", self.session_id.to_string()); // missing Accept + OpenAI-Beta
```

- Don’t rely on live endpoints in tests: Always use a mock server and fixtures for deterministic behavior.
```rust
// WRONG: hitting real endpoints makes tests flaky and slow
let base_url = "https://api.openai.com/v1"; // avoid in tests
```

- Don’t write tests that can hang: Wrap event waits with a timeout to ensure progress.
```rust
// WRONG: can hang indefinitely
let ev = codex.next_event().await.unwrap();

// RIGHT: guard with a timeout
let ev = tokio::time::timeout(Duration::from_secs(1), codex.next_event())
    .await.unwrap().unwrap();
```