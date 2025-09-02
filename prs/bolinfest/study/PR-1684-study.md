**DOs**

- **Use strum Display with camelCase:** Derive `Display` for `EventMsg` and set `#[strum(serialize_all = "camelCase")]` so `event.msg.to_string()` yields MCP-friendly method names.
```rust
use serde::{Deserialize, Serialize};
use strum_macros::Display;

#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(tag = "type", rename_all = "snake_case")] // old schema tag values
#[strum(serialize_all = "camelCase")]             // new schema method names
pub enum EventMsg {
    SessionConfigured,
    ShutdownComplete,
    // ...
}
```

- **Emit both schemas during transition:** Send the legacy `"codex/event"` and the new per-event method to stay backward compatible.
```rust
let params = Some(serde_json::to_value(event)?);

// Old schema
self.sender.send(OutgoingMessage::Notification(OutgoingNotification {
    method: "codex/event".to_string(),
    params: params.clone(),
})).await.ok();

// New schema (method from enum Display)
self.sender.send(OutgoingMessage::Notification(OutgoingNotification {
    method: event.msg.to_string(), // e.g., "sessionConfigured"
    params,
})).await.ok();
```

- **Parse both in tests and assert consistency:** Accept either method and ensure the same `session_id` is seen in both.
```rust
let mut sid_old: Option<String> = None;
let mut sid_new: Option<String> = None;

if let JSONRPCMessage::Notification(n) = message {
    if let Some(p) = n.params {
        if n.method == "codex/event" {
            if p.get("msg")
                .and_then(|m| m.get("type")).and_then(|t| t.as_str())
                == Some("session_configured")
            {
                sid_old = p.get("msg")
                    .and_then(|m| m.get("session_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
        if n.method == "sessionConfigured" {
            sid_new = p.get("msg")
                .and_then(|m| m.get("session_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    }
}

if sid_old.is_some() && sid_new.is_some() {
    assert_eq!(sid_old.as_ref().unwrap(), sid_new.as_ref().unwrap());
}
```

- **Use a single source of truth for method names:** Always call `event.msg.to_string()` instead of repeating string literals.
```rust
let method = event.msg.to_string(); // good
// let method = "sessionConfigured".to_string(); // avoid
```

- **Override per-variant names when needed:** Use strum variant attributes to pin specific spellings.
```rust
use strum_macros::Display;

#[derive(Display)]
#[strum(serialize_all = "camelCase")]
enum EventMsg {
    #[strum(serialize = "sessionReady")] // custom override
    SessionConfigured,
    ToolRegistered,
}
```

- **Make the new params shape explicit:** Prefer typed params for the new schema to make intent clear.
```rust
use serde::Serialize;

#[derive(Serialize)]
struct SessionConfiguredParams<'a> {
    session_id: &'a str,
}

// Later
self.sender.send(OutgoingMessage::Notification(OutgoingNotification {
    method: "sessionConfigured".into(),
    params: Some(serde_json::to_value(SessionConfiguredParams { session_id })?),
})).await.ok();
```


**DON’Ts**

- **Don’t use lowercase method names:** Avoid `"sessionconfigured"`. MCP favors camelCase like `"sessionConfigured"`.
```rust
// BAD
let method = "sessionconfigured";
// GOOD
let method = "sessionConfigured";
```

- **Don’t hand-roll fmt::Display impls:** Use `#[derive(Display)]` from strum instead of custom `impl fmt::Display`.
```rust
// Avoid manual impl; derive instead
#[derive(Display)]
#[strum(serialize_all = "camelCase")]
enum EventMsg { /* ... */ }
```

- **Don’t collapse schemas into one ambiguous path:** Keep legacy `"codex/event"` and new per-event methods distinct; don’t replace one with the other prematurely.
```rust
// Avoid: only emitting "codex/event" or only the new method during transition
// Do: emit both until consumers migrate
```

- **Don’t assume params stay the same:** The new schema may change `params`. Don’t lock into `serde_json::Value` forever—introduce typed structs as they stabilize.
```rust
// Avoid long-term:
let params: Option<serde_json::Value> = Some(serde_json::to_value(event)?);

// Prefer:
#[derive(Serialize)] struct ShutdownCompleteParams;
```

- **Don’t duplicate magic strings in code/tests:** Don’t repeat method names in multiple places; derive from the enum to prevent drift.
```rust
assert_eq!(event.msg.to_string(), "sessionConfigured"); // one reference
```

- **Don’t introduce incidental changes:** Avoid unrelated edits (formatting, stray removals). Keep diffs focused so reviewers can disambiguate intentional protocol changes.