**PR #2496 Review Takeaways**

**DOs**
- **Model Notifications As Enums:** Define a typed `ServerNotification` (parallel to `ClientRequest`) and let serde drive the wire shape.
```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
pub enum ServerNotification {
    AuthStatusChange { params: AuthStatusChangeNotification },
    LoginChatGptComplete { params: LoginChatGptCompleteNotification },
}
```

- **Send Typed Notifications:** Add a helper that accepts `ServerNotification` directly; avoid manual JSON plumbing.
```rust
impl OutgoingMessageSender {
    pub async fn send_server_notification(&self, n: ServerNotification) {
        let json = serde_json::to_value(&n).expect("serialize notification");
        let method = json.get("method").and_then(|v| v.as_str()).unwrap().to_string();
        let params = json.get("params").cloned();
        let msg = OutgoingMessage::Notification(OutgoingNotification { method, params });
        let _ = self.sender.send(msg).await;
    }
}
```

- **Derive Config Early:** Parse CLI overrides once, produce a `Config`, and pass `Arc<Config>` down to processors.
```rust
use std::io::{Error, ErrorKind};
use std::sync::Arc;

pub async fn run_main(sandbox: Option<PathBuf>, cli: CliConfigOverrides) -> IoResult<()> {
    let kv = cli.parse_overrides()
        .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("error parsing -c overrides: {e}")))?;
    let config = Config::load_with_cli_overrides(kv, ConfigOverrides::default())
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("error loading config: {e}")))?;

    let outgoing = OutgoingMessageSender::new(outgoing_tx);
    let mut processor = MessageProcessor::new(outgoing, sandbox, Arc::new(config));
    // ...
    Ok(())
}
```

- **Keep Dependency Direction Clean:** Put shared auth types in `codex-protocol`; have `login` depend on `protocol`, not vice versa.
```toml
# login/Cargo.toml
[dependencies]
codex-protocol = { path = "../protocol" }

# login/src/lib.rs
pub use codex_protocol::mcp_protocol::AuthMode;
```

- **Scope Locks To Drop Guards:** Prefer scoped blocks over explicit `drop(guard)`.
```rust
{
    let mut guard = self.active_login.lock().await;
    if let Some(active) = guard.take() {
        active.drop();
    }
}
```

- **Emit Auth Events On State Changes:** Notify on successful login and on logout.
```rust
// After successful login
let payload = AuthStatusChangeNotification { auth_method: Some(AuthMode::ChatGPT) };
outgoing.send_server_notification(ServerNotification::AuthStatusChange { params: payload }).await;

// After logout
let payload = AuthStatusChangeNotification { auth_method: None };
outgoing.send_server_notification(ServerNotification::AuthStatusChange { params: payload }).await;
```

- **Use `From`/`Into` When Conversions Are Needed:** If you still convert to `OutgoingNotification`, implement standard traits.
```rust
impl From<ServerNotification> for OutgoingNotification {
    fn from(n: ServerNotification) -> Self {
        let v = serde_json::to_value(n).unwrap();
        let method = v["method"].as_str().unwrap().to_string();
        let params = v.get("params").cloned();
        OutgoingNotification { method, params }
    }
}
// usage: sender.send_notification(ServerNotification::AuthStatusChange { params }.into()).await;
```

- **Prefer `map_err` + `?` For Errors:** Keep error paths concise and readable.
```rust
let kv = cli.parse_overrides()
    .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("error parsing -c overrides: {e}")))?;
```

- **Update TS Codegen When Adding Types:** Export new enums/structs in `protocol-ts`.
```rust
pub fn generate_ts(out_dir: &Path, prettier: Option<&Path>) -> Result<()> {
    codex_protocol::mcp_protocol::ServerNotification::export_all_to(out_dir)?;
    // ...
    Ok(())
}
```

- **Align Client/Server Shapes:** Keep `ClientRequest` variants and handler signatures consistent (params presence, names, and serde tags).
```rust
// Protocol
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum ClientRequest {
    GetAuthStatus { #[serde(rename = "id")] request_id: RequestId },
}

// Server
match req {
    ClientRequest::GetAuthStatus { request_id } => self.get_auth_status(request_id).await,
    // ...
}
```

**DON’Ts**
- **Don’t Depend `protocol` On `login`:** Avoid `codex-login` in `protocol/Cargo.toml`; move shared types into `protocol` instead.
```toml
# ❌ protocol/Cargo.toml (wrong)
[dependencies]
codex-login = { path = "../login" }
```

- **Don’t Handcraft Method Strings Or Use Event Constants:** Replace stringly-typed `"codex/event/..."/LOGIN_*` patterns with typed enums.
```rust
// ❌ Avoid
outgoing.send_notification(OutgoingNotification {
    method: "codex/event/login_chatgpt_complete".to_string(),
    params: Some(serde_json::to_value(&payload).unwrap()),
}).await;
```

- **Don’t Thread Raw CLI Override Types Downstream:** Keep `TomlValue`/override maps out of processors; pass `Arc<Config>` instead.
```rust
// ❌ Avoid
let mut processor = MessageProcessor::new(outgoing, sandbox, cli_kv_overrides);
```

- **Don’t Duplicate Auth Enums Across Crates:** Use the single source of truth in `codex-protocol`.
```rust
// ❌ Avoid redefining
#[derive(Serialize, Deserialize)]
enum AuthMode { ApiKey, ChatGPT }
```

- **Don’t Rely On Explicit `drop(guard)`:** End the scope to release locks predictably.
```rust
// ❌ Avoid
let mut guard = self.active_login.lock().await;
// ...
drop(guard);
```

- **Don’t Invent Custom Conversion Traits:** Prefer `From`/`Into` over bespoke traits like `IntoOutgoingNotification`.
```rust
// ❌ Avoid
pub trait IntoOutgoingNotification { fn into_outgoing_notification(self) -> OutgoingNotification; }
```

- **Don’t Bake In A `codex/event` Prefix:** Use serde-tagged enums (`method`/`params`) rather than string concatenation.
```rust
// ❌ Avoid
let method = format!("codex/event/{}", notification);
```

- **Don’t Forget Feature-Gating TS Derives (When Needed):** If `TS` leaks into crates that shouldn’t depend on `ts-rs`, gate it.
```toml
# protocol/Cargo.toml
[features]
ts = ["ts-rs"]

# protocol/src/...
#[cfg_attr(feature = "ts", derive(TS))]
```

- **Don’t Let Client/Server Drift:** Avoid mismatched request params (e.g., protocol expects `params`, server ignores them).
```rust
// ❌ Avoid: protocol defines params but server handler takes none
GetAuthStatus { request_id, params: GetAuthStatusParams }
```