**DOs**

- **Return Named Structs**: Replace multi-value tuple returns with small, public structs.
```rust
// Before
pub async fn spawn(config: Config, ctrl_c: Arc<Notify>) -> CodexResult<(Codex, String, Uuid)>;

// After
pub struct CodexSpawnOk {
    pub codex: Codex,
    pub init_id: String,
    pub session_id: Uuid,
}
pub async fn spawn(config: Config, ctrl_c: Arc<Notify>) -> CodexResult<CodexSpawnOk>;
```

- **Export New Types**: Re-export structs from the crate root to stabilize the public API.
```rust
// codex-rs/core/src/lib.rs
pub use codex::CodexSpawnOk;
```

- **Use Clear “Ok” Naming**: Name Ok-payload types with an `Ok` suffix when returned inside `Result<T, E>`.
```rust
pub struct CodexSpawnOk { /* ... */ } // Good: represents the Ok variant payload
```

- **Destructure With `..`**: Pull only the fields you need and ignore the rest succinctly.
```rust
let CodexSpawnOk { codex, .. } = Codex::spawn(config, ctrl_c.clone()).await?;
```

- **Prefer `init_codex()`**: For high-level callers, use the wrapper that validates the first event and returns a cohesive conversation handle.
```rust
use codex_core::codex_wrapper::{init_codex, CodexConversation};

let CodexConversation {
    codex,
    session_configured,
    ctrl_c,
    session_id,
} = init_codex(config).await?;
```

- **Forward the Initial Event**: Surface the validated `SessionConfigured` event to UIs/clients immediately.
```rust
// MCP server example
outgoing.send_event_as_notification(&session_configured).await;

// TUI example
app_event_tx.send(AppEvent::CodexEvent(session_configured.clone()));
```

- **Inline Format Variables**: Use `{var}` directly in `format!` and logging macros.
```rust
info!("resume_path: {resume_path:?}");
info!("Codex initialized with event: {session_configured:?}");
```

**DON’Ts**

- **Don’t Return Tuples**: Avoid ambiguous, brittle tuple returns for multi-value results.
```rust
// Avoid
pub async fn spawn(...) -> CodexResult<(Codex, String, Uuid)>;
```

- **Don’t Use “Result” In Type Names**: If a type is the Ok payload (not a `Result` itself), don’t name it `...Result`.
```rust
// Avoid
pub struct CodexSpawnResult { /* ... */ }
```

- **Don’t Re‑validate First Event After `init_codex()`**: It already checks that the first event is `EventMsg::SessionConfigured`.
```rust
// Avoid re-checking
match &session_configured.msg {
    EventMsg::SessionConfigured { .. } => { /* redundant */ }
    _ => unreachable!(),
}
```

- **Don’t Bind Unused Fields**: Skip needless variables; use `..` in struct patterns.
```rust
// Avoid
let CodexSpawnOk { codex, init_id, session_id } = Codex::spawn(config, ctrl_c).await?;
// Prefer
let CodexSpawnOk { codex, .. } = Codex::spawn(config, ctrl_c).await?;
```

- **Don’t Drop The Initial Event**: Ensure the captured `session_configured` is forwarded; don’t silently ignore it.
```rust
// Avoid: never sending the initial event to the client/UI
// Correct: always send `session_configured` as shown in the DOs above.
```