**DOs**
- Assert full structures: compare entire streams and payloads for exactness.
```rust
use futures::stream;

let events = vec![
    Ok(text_chunk("Hello")),
    Ok(text_chunk(", world")),
    Ok(ResponseEvent::Completed { response_id: "r1".into(), token_usage: None }),
];

let collected: Vec<_> = stream::iter(events).aggregate().map(Result::unwrap).collect().await;

let expected = vec![
    ResponseEvent::OutputItemDone(ResponseItem::Message {
        role: "assistant".into(),
        content: vec![ContentItem::OutputText { text: "Hello, world".into() }],
    }),
    ResponseEvent::Completed { response_id: "r1".into(), token_usage: None },
];

assert_eq!(collected, expected, "aggregated events mismatch");
```

- Derive PartialEq on model types to enable simple equality assertions.
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResponseItem {
    Message { role: String, content: Vec<ContentItem> },
    // ...
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContentItem {
    InputText { text: String },
    OutputText { text: String },
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct FunctionCallOutputPayload {
    pub content: String,
    #[allow(dead_code)]
    pub success: Option<bool>,
}
```

- Assert JSON request bodies end‑to‑end (not just subfields).
```rust
use serde_json::json;

let expected_body = json!({
    "model": "model",
    "messages": expected_messages,
    "stream": true,
    "tools": tools_json,
});

assert_eq!(body, expected_body, "chat payload encoded incorrectly");
```

- Inject configuration instead of mutating global env; pass a provider with a mock base URL.
```rust
let provider = ModelProviderInfo {
    name: "openai".into(),
    base_url: format!("{}/v1", server.uri()),
    env_key: Some("PATH".into()),
    env_key_instructions: None,
    wire_api: crate::WireApi::Chat,
    query_params: None,
    http_headers: None,
    env_http_headers: None,
};

// Use the provider directly in the call under test.
let _ = stream_chat_completions(&prompt, "model", &client, &provider).await.unwrap();
```

- Prefer single, definitive assertions over multiple piecemeal checks.
```rust
// Good: one precise check
assert_eq!(collected, expected);
```

**DON’Ts**
- Don’t only check lengths or partial shapes; avoid weak assertions.
```rust
// Avoid
assert_eq!(collected.len(), 2);
matches!(collected[0], ResponseEvent::OutputItemDone(_));
```

- Don’t cherry‑pick nested fields from captured JSON when validating payloads.
```rust
// Avoid
let messages = body.get("messages").unwrap();
// ...assert only parts of messages...
```

- Don’t use unsafe env mutations in tests; remove global state dependencies.
```rust
// Avoid
unsafe { std::env::set_var("OPENAI_REQUEST_MAX_RETRIES", "0"); }
```

- Don’t granularly match intermediate pieces when you can compare complete, derived‑equatable values.
```rust
// Avoid
if let ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) = &collected[0] {
    // manually dig into content[0]...
}
```