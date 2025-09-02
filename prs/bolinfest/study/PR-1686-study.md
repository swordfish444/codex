**PR 1686 Review Takeaways — bolinfest**

**DOs**
- Log channel send failures and continue: treat as “receiver closed.”
```rust
if let Err(e) = sender.send(outgoing).await {
    tracing::warn!("Dropping response; client disconnected or closed: {e}");
}
```

- Keep response-sending helpers side-effecting; don’t return Result from them.
```rust
pub(crate) async fn send_response_with_optional_error(
    &self,
    id: RequestId,
    message: Option<ToolCallResponseResult>,
    error: Option<bool>,
) {
    let response = ToolCallResponse {
        request_id: id.clone(),
        is_error: error,
        result: message,
    };
    let result: CallToolResult = response.into();
    self.send_response::<mcp_types::CallToolRequest>(id, result).await;
}
```

- Prefer specific error types only when propagation is meaningful.
```rust
#[derive(Debug)]
enum SendErr { Closed }

async fn try_send(sender: &tokio::sync::mpsc::Sender<OutgoingMessage>, m: OutgoingMessage)
    -> Result<(), SendErr>
{
    sender.send(m).await.map_err(|_| SendErr::Closed)
}
```

- Keep conversions conventional: convert response → result, pass `id` explicitly.
```rust
let response = ToolCallResponse { request_id: id.clone(), is_error, result: payload };
let result: CallToolResult = response.into();
self.send_response::<mcp_types::CallToolRequest>(id, result).await;
```

- Use inline formatting consistently for logs/messages.
```rust
let session = session_id;
tracing::info!("Submitting user input for session {session}");
```

**DON’Ts**
- Don’t bubble `mpsc::Sender::send` errors as `anyhow::Result` from helpers.
```rust
// Avoid
pub async fn send_response(...) -> anyhow::Result<()> { /* ... */ }
```

- Don’t attempt recovery loops on a closed receiver; there’s nothing meaningful to do.
```rust
// Avoid retry loops; just log and return.
```

- Don’t force “Result all the way up” when callers can’t act on it.
```rust
// Avoid turning a fire-and-forget send into a chained anyhow::Result.
```

- Don’t create clever `From/Into` that return tuples or compound outputs.
```rust
// Avoid
let (result, id): (CallToolResult, RequestId) = response.into();

// Prefer
let result: CallToolResult = response.into();
```