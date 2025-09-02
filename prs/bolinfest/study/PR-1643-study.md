**DOs**
- Return a struct instead of a long tuple for multi-value results.
```rust
// Before
pub async fn spawn(config: Config, ctrl_c: Arc<Notify>) -> CodexResult<(Codex, String, Uuid)> { ... }

// After
pub struct SpawnResult {
    pub codex: Codex,
    pub init_event_id: String,
    pub session_id: Uuid,
}

pub async fn spawn(config: Config, ctrl_c: Arc<Notify>) -> CodexResult<SpawnResult> {
    // ...
    Ok(SpawnResult { codex, init_event_id: init_id, session_id })
}

// Call site
let SpawnResult { codex, init_event_id, session_id } = Codex::spawn(config, ctrl_c).await?;
```

- Keep parameters immutable; only create a mutable local when truly necessary.
```rust
// Prefer immutable params
async fn submission_loop(session_id: Uuid, /* ... */) {
    // If you must mutate, make a local copy
    let mut current_session = session_id;
    // ...
}
```

- Use camelCase for MCP tool schemas with serde’s rename_all.
```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CodexToolCallReplyParam {
    /// The *session id* for this conversation.
    session_id: String,
    /// The *next user prompt* to continue the Codex conversation.
    prompt: String,
}
```

- Release Mutex guards promptly in async code; don’t hold them across long operations.
```rust
let mut map = session_map.lock().await;
map.insert(session_id, codex.clone());
drop(map); // release before streaming loop
// Now do long-running work without the lock held
run_codex_tool_session_inner(codex, outgoing, request_id).await;
```

- Always send an error response to the client on failures; use the same request_id you received.
```rust
let result = CallToolResult {
    content: vec![ContentBlock::TextContent(TextContent {
        r#type: "text".into(),
        text: format!("Unknown tool '{name}'"),
        annotations: None,
    })],
    is_error: Some(true),
    structured_content: None,
};
outgoing
    .send_response::<mcp_types::CallToolRequest>(request_id.clone(), result)
    .await;
```

- Convert RequestId to a String only when needed (e.g., for submission IDs); keep using RequestId for responses.
```rust
let sub_id = match &request_id {
    RequestId::String(s) => s.clone(),
    RequestId::Integer(n) => n.to_string(),
};
// Use `sub_id` internally, but reply with `request_id`
outgoing.send_response::<mcp_types::CallToolRequest>(request_id.clone(), result).await;
```

**DON'Ts**
- Don’t grow tuples of return values; avoid unnamed return values that cause churn.
```rust
// Avoid
pub async fn spawn(/* ... */) -> CodexResult<(Codex, String, Uuid)> { /* ... */ }
```

- Don’t mark parameters mut unless you reassign them; prefer local mutable copies if needed.
```rust
// Avoid
async fn submission_loop(mut session_id: Uuid, /* ... */) { /* ... */ }
```

- Don’t hold a MutexGuard across awaits or long-running loops; it risks contention or deadlocks.
```rust
// Avoid: guard held while streaming events
let guard = session_map.lock().await;
run_codex_tool_session_inner(codex, outgoing, request_id).await; // guard not dropped
```

- Don’t use kebab-case (or snake_case) in MCP JSON schemas.
```rust
// Avoid
#[serde(rename_all = "kebab-case")]
struct CodexToolCallReplyParam { /* ... */ }
```

- Don’t just log errors and return; always respond so the client/LLM can react.
```rust
// Avoid
tracing::error!("Failed to parse params: {e}");
return; // no response sent
```

- Don’t send responses with a stale or unrelated id; always reply with the original request_id.
```rust
// Avoid
outgoing.send_response(id.clone(), result.into()).await; // `id` is not the current request_id
```