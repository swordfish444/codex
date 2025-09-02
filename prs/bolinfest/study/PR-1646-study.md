**DOs**

- **Use one-liner mutex ops**: Acquire the lock, do the operation, and let the guard drop immediately.
```rust
running_requests_id_to_codex_uuid
    .lock()
    .await
    .insert(request_id.clone(), session_id);

// ...

running_requests_id_to_codex_uuid
    .lock()
    .await
    .remove(&request_id);
```

- **Handle error events decisively**: On `EventMsg::Error`, return a response, unregister, and break the loop.
```rust
match event {
    EventMsg::Error(err) => {
        outgoing
            .send_response(request_id.clone(), serde_json::json!({ "error": err.message }))
            .await;
        running_requests_id_to_codex_uuid.lock().await.remove(&request_id);
        break;
    }
    _ => { /* ... */ }
}
```

- **Make notifications truly async**: If the handler awaits, mark it `async` and await at the call site.
```rust
// lib.rs
match msg {
    JSONRPCMessage::Notification(n) => processor.process_notification(n).await,
    _ => { /* ... */ }
}

// message_processor.rs
pub(crate) async fn process_notification(&mut self, n: JSONRPCNotification) { /* ... */ }
```

- **Minimize lock scope and avoid nested locks**: Read what you need, drop the guard, then lock the next map.
```rust
let session_id = {
    let g = self.running_requests_id_to_codex_uuid.lock().await;
    match g.get(&request_id).copied() { Some(id) => id, None => return }
};

let codex = {
    let g = self.session_map.lock().await;
    match g.get(&session_id).cloned() { Some(c) => c, None => return }
};
```

- **Defer derived data until used**: Only compute `request_id_string` where you actually need it.
```rust
let maybe_id = self.running_requests_id_to_codex_uuid.lock().await.get(&request_id).copied();
if maybe_id.is_none() {
    let request_id_string = match &request_id {
        RequestId::String(s) => s.clone(),
        RequestId::Integer(i) => i.to_string(),
    };
    tracing::warn!("Session not found for request_id: {request_id_string}");
    return;
}
```

- **Write precise, portable tests**: Assert JSON-RPC errors explicitly and make blocking commands cross‑platform.
```rust
// Match JSON-RPC error
match mcp_process.read_jsonrpc_message().await? {
    JSONRPCMessage::Error(e) if e.id == RequestId::Integer(codex_request_id) => { /* ok */ }
    other => anyhow::bail!("unexpected message: {other:?}"),
}

// Cross-platform blocking command
#[cfg(target_os = "windows")]
let shell_command = vec![
    "powershell".to_string(), "-Command".to_string(), "Start-Sleep -Seconds 60".to_string()
];
#[cfg(not(target_os = "windows"))]
let shell_command = vec!["sleep".to_string(), "60".to_string()];
```

**DON’Ts**

- **Don’t hold locks longer than necessary**: Avoid temporary guard variables and extra braces that extend lifetimes.
```rust
// ❌ Holds the lock longer than needed
let mut guard = running.lock().await;
guard.insert(request_id.clone(), session_id);
// guard lives until end of scope
```

- **Don’t ignore cleanup on all exit paths**: Always unregister request IDs on success, error, and submit failures.
```rust
// ❌ Missing cleanup on error
if let Err(e) = codex.submit_with_id(submission).await {
    tracing::error!("submit failed: {e}");
    // running_requests_id_to_codex_uuid.remove(...) is missing
}
```

- **Don’t treat MCP errors as “responses”**: Don’t assert on `JSONRPCResponse` with null/embedded error; match `JSONRPCMessage::Error`.
```rust
// ❌ Fragile: assumes error tunneled as a response payload
let JSONRPCMessage::Response(r) = msg else { /* ... */ };
assert!(r.result.get("error").is_some());
```

- **Don’t precompute derived strings you may not use**: Compute `request_id_string` only in the branches that need it.
```rust
// ❌ Work done upfront even if early-return
let request_id_string = match &request_id { /* ... */ };
// early return before using request_id_string
```

- **Don’t blanket-allow dead code/imports**: Prefer targeted allowances or remove unused helpers.
```rust
// ❌ Blanket allow across a module
#![allow(dead_code, unused_imports)]

// ✅ Narrow allowance for a single helper used by some tests
#[allow(dead_code)]
async fn read_stream_until_error(/* ... */) { /* ... */ }
```