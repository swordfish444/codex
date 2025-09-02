**DOs**

- Use strong types for API fields: prefer enums over strings for request/response parameters.
```rust
use codex_core::protocol::AskForApproval;
use codex_core::config_types::SandboxMode;

#[derive(Serialize, Deserialize)]
pub struct NewConversationArgs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    pub model: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<AskForApproval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxMode>,
}
```

- Newtype all identifiers: wrap raw IDs (e.g., `u32`/`Uuid`) so the representation can change without touching call sites.
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConversationId(pub u32); // swap to Uuid later without churn

#[derive(Serialize, Deserialize)]
pub struct ConnectArgs {
    pub conversation_id: ConversationId,
}
```

- Convert ID representations at boundaries: accept one form, respond with another if desired.
```rust
// Example: accept Uuid on input, store/map to u32 internally, return ConversationId(u32)
fn register_conversation(external: uuid::Uuid) -> ConversationId {
    let short = id_map_insert_or_get_u32(external); // app logic, not shown
    ConversationId(short)
}
```

- Co-locate requests with their results: keep related types adjacent and consistently named.
```rust
#[derive(Serialize, Deserialize)]
pub struct ConnectArgs {
    pub conversation_id: ConversationId,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ConnectResult; // empty result lives next to ConnectArgs

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ToolCallResponseData {
    Connect(ConnectResult),
    // ...
}
```

- Use consistent “Result” suffix for response payloads: avoid mixed naming like “Accepted.”
```rust
#[derive(Serialize, Deserialize)]
pub struct SendUserMessageResult {
    pub accepted: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ToolCallResponseData {
    SendUserMessage(SendUserMessageResult),
    // ...
}
```

- Unify notifications under one enum and dispatch by a stable tag: one place to match on `type`.
```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ServerNotification {
    InitialState(InitialStateNotificationParams),
    ConnectionRevoked(ConnectionRevokedNotificationParams),
    CodexEvent(CodexEventNotificationParams),
    Cancelled(CancelledNotificationParams),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<ConversationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<mcp_types::RequestId>,
}
```

- Omit empties and nulls in JSON: use serde defaults and `skip_serializing_if`.
```rust
#[derive(Serialize, Deserialize)]
pub struct InitialStatePayload {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<CodexEventNotificationParams>,
}

#[derive(Serialize, Deserialize)]
pub struct CancelledNotificationParams {
    pub id: mcp_types::RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
```

- Favor clear test naming and ergonomics: use `observed`/`expected` and `.expect(...)`.
```rust
#[test]
fn serialize_initial_state_minimal() {
    let params = InitialStateNotificationParams { /* ... */ };
    let observed = serde_json::to_value(&params)
        .expect("serialize InitialStateNotificationParams");
    let expected = serde_json::json!({ /* exact shape */ });
    assert_eq!(observed, expected);
}
```

**DON’Ts**

- Don’t use `String` for typed fields: avoid `Option<String>` for things like approval policy or sandbox.
```rust
// ❌ Avoid
pub struct NewConversationArgs {
    pub approval_policy: Option<String>,
    pub sandbox: Option<String>,
}
```

- Don’t pass raw primitives for IDs throughout the codebase: avoid leaking `Uuid`/`u32` directly.
```rust
// ❌ Avoid
pub struct ConnectArgs {
    pub conversation_id: uuid::Uuid,
}
```

- Don’t mix response naming patterns: avoid one-offs like `SendUserMessageAccepted` among `*Result` types.
```rust
// ❌ Avoid
pub enum ToolCallResponseData {
    SendUserMessage(SendUserMessageAccepted),
}
```

- Don’t fragment notification handling across multiple enums: avoid scattering and ad-hoc dispatch.
```rust
// ❌ Avoid
pub enum ConversationNotificationParams { /* subset only */ }
pub enum SystemNotificationParams { /* another subset */ }
```

- Don’t write verbose test error handling or ambiguous variables: avoid `got` and manual `match` on results.
```rust
// ❌ Avoid
let got = match serde_json::to_value(&params) {
    Ok(v) => v,
    Err(e) => panic!("failed: {e}"),
};
```

- Don’t serialize empty/default fields: avoid emitting `reason: null` or empty arrays by default.
```rust
// ❌ Avoid
#[derive(Serialize)]
pub struct CancelledNotificationParams {
    pub id: mcp_types::RequestId,
    pub reason: Option<String>, // will serialize as null without skip_serializing_if
}
```