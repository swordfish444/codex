**DOs**
- Enable elicitation capability: Advertise support with an empty object per spec.
```rust
use serde_json::json;
use mcp_types::ClientCapabilities;

let capabilities = ClientCapabilities {
    experimental: None,
    roots: None,
    sampling: None,
    // Spec requires an empty object when enabled.
    elicitation: Some(json!({})),
};
```

- Escape commands for display: Quote arguments safely before prompting users.
```rust
use shlex;
let escaped_command = shlex::try_join(command.iter().map(|s| s.as_str()))
    .unwrap_or_else(|_| command.join(" "));
let message = format!("Allow Codex to run `{}` in {:?}?", escaped_command, cwd);
```

- Send MCP elicitation requests and await responses off-thread: Return a oneshot receiver from `send_request()` and `tokio::spawn` a task to wait for it.
```rust
use mcp_types::ElicitRequest;
use serde_json::json;

let params = json!({
    "message": message,
    "requestedSchema": { "type": "object", "properties": {} },
    // Correlation helpers:
    "codex_elicitation": "exec-approval",
    "codex_mcp_tool_call_id": sub_id.clone(),
    "codex_event_id": event.id.clone(),
    "codex_command": command,
    "codex_cwd": cwd.to_string_lossy(),
});

let rx = outgoing.send_request(ElicitRequest::METHOD, Some(params)).await;
let codex = codex.clone();
let event_id = event.id.clone();
tokio::spawn(async move {
    on_exec_approval_response(event_id, rx, codex).await;
});
```

- Correlate JSON-RPC requests with responses: Store a callback per `RequestId`, remove it on response, and notify the waiter.
```rust
use std::{collections::HashMap, sync::atomic::{AtomicI64, Ordering}};
use tokio::sync::{mpsc, oneshot, Mutex};
use mcp_types::{RequestId, Result};

struct OutgoingMessageSender {
    next_request_id: AtomicI64,
    sender: mpsc::Sender<OutgoingMessage>,
    request_id_to_callback: Mutex<HashMap<RequestId, oneshot::Sender<Result>>>,
}

impl OutgoingMessageSender {
    async fn send_request(&self, method: &str, params: Option<serde_json::Value>)
        -> oneshot::Receiver<Result> {
        let id = RequestId::Integer(self.next_request_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        self.request_id_to_callback.lock().await.insert(id.clone(), tx);
        let _ = self.sender.send(OutgoingMessage::Request(OutgoingRequest {
            id, method: method.to_string(), params
        })).await;
        rx
    }

    async fn notify_client_response(&self, id: RequestId, result: Result) {
        if let Some((_id, tx)) = self.request_id_to_callback.lock().await.remove_entry(&id) {
            let _ = tx.send(result);
        }
    }
}
```

- Make response handling async end-to-end: Let the processor await notifying callbacks.
```rust
pub(crate) async fn process_response(&mut self, response: JSONRPCResponse) {
    let JSONRPCResponse { id, result, .. } = response;
    self.outgoing.notify_client_response(id, result).await;
}
```

- Ensure hashability for map keys: Derive `Hash` and `Eq` for `RequestId` (and other untagged enums used as keys).
```rust
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Hash, Eq)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Integer(i64),
}
```

- Clone IDs you still need: Avoid moving `sub_id` into a struct if used later.
```rust
let submission = Submission {
    id: sub_id.clone(),
    op: Op::UserInput { items: vec![InputItem::Text { text: prompt, annotations: None }] },
};
```

- Validate and forward elicitation decisions: Deserialize the client response and submit `Op::ExecApproval`.
```rust
#[derive(Deserialize)]
struct ExecApprovalResponse { decision: ReviewDecision }

async fn on_exec_approval_response(
    event_id: String,
    rx: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    codex: std::sync::Arc<Codex>,
) {
    let value = match rx.await {
        Ok(v) => v,
        Err(err) => { tracing::error!("request failed: {err:?}"); return; }
    };
    let resp: ExecApprovalResponse = match serde_json::from_value(value) {
        Ok(r) => r,
        Err(err) => { tracing::error!("failed to deserialize ExecApprovalResponse: {err}"); return; }
    };
    if let Err(err) = codex.submit(Op::ExecApproval { id: event_id, decision: resp.decision }).await {
        tracing::error!("failed to submit ExecApproval: {err}");
    }
}
```

**DON’Ts**
- Block the event loop waiting for approval: Don’t `.await` the elicitation response inline where events are processed.
```rust
// ❌ Avoid blocking here:
// let result = outgoing.send_request(...).await; result.await; // blocks the loop

// ✅ Instead, spawn a task to await:
let rx = outgoing.send_request(...).await;
tokio::spawn(async move { on_exec_approval_response(event_id, rx, codex).await; });
```

- Forget to remove callbacks on response: Don’t leave entries in the `HashMap` and leak memory.
```rust
// ❌ Wrong: leaving the sender in the map
// self.request_id_to_callback.lock().await.get(&id).unwrap().send(result);

// ✅ Right: remove then send
if let Some((_id, tx)) = self.request_id_to_callback.lock().await.remove_entry(&id) {
    let _ = tx.send(result);
}
```

- Use non-hashable IDs as map keys: Don’t omit `Hash`/`Eq` on enums stored in `HashMap`.
```rust
// ❌ Missing Hash/Eq will fail at compile time

// ✅ Derive Hash/Eq
#[derive(Hash, Eq, PartialEq, Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum RequestId { String(String), Integer(i64) }
```

- Display raw unquoted commands: Don’t show `["git", "commit", "-m", "a b"]` as `git commit -m a b`.
```rust
// ❌ Raw join can be misleading:
let msg = format!("Run {}?", command.join(" "));

// ✅ Quote safely:
let msg = format!("Run `{}`?", shlex::try_join(command.iter().map(|s| s.as_str()))
    .unwrap_or_else(|_| command.join(" ")));
```

- Misdeclare elicitation capability: Don’t set `elicitation: None` or a non-empty schema object.
```rust
// ❌ Wrong:
let capabilities = ClientCapabilities { elicitation: None, /* ... */ };

// ❌ Also wrong (non-empty custom fields):
let capabilities = ClientCapabilities { elicitation: Some(json!({"foo":"bar"})), /* ... */ };

// ✅ Correct (empty object):
let capabilities = ClientCapabilities { elicitation: Some(serde_json::json!({})), /* ... */ };
```

- Move IDs you still need: Don’t consume `sub_id` or `event.id` if referenced later.
```rust
// ❌ Moved into struct, unusable later:
// let submission = Submission { id: sub_id, /* ... */ }; use(sub_id); // error

// ✅ Clone before move:
let submission = Submission { id: sub_id.clone(), /* ... */ }; use(sub_id);
```

- Ignore error paths from channels/deserialization: Don’t unwrap; log and return gracefully.
```rust
// ❌ Panics on failure:
// let value = rx.await.unwrap();
// let resp: ExecApprovalResponse = serde_json::from_value(value).unwrap();

// ✅ Robust handling:
let value = match rx.await { Ok(v) => v, Err(e) => { tracing::error!("{e:?}"); return; } };
let resp: ExecApprovalResponse = match serde_json::from_value(value) {
    Ok(r) => r, Err(e) => { tracing::error!("{e}"); return; }
};
```