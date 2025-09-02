**MCP Server Name Validation — Review Takeaways**

**DOs**
- Validate early: Check names against ^[a-zA-Z0-9_-]+$ before spawning; skip invalid.
```rust
let mut join_set = JoinSet::new();
let mut errors = ClientStartErrors::new();

for (server_name, cfg) in mcp_servers {
    if !is_valid_mcp_server_name(&server_name) {
        let msg = anyhow::anyhow!(
            "invalid server name '{}': must match pattern ^[a-zA-Z0-9_-]+$",
            server_name
        );
        errors.insert(server_name, msg);
        continue;
    }

    join_set.spawn(async move {
        let McpServerConfig { command, args, env } = cfg;
        let client_res = McpClient::new_stdio_client(command, args, env).await;
        (server_name, client_res)
    });
}
```

- Keep helpers local: Define private, domain-specific validator in `mcp_connection_manager.rs`.
```rust
// core/src/mcp_connection_manager.rs
fn is_valid_mcp_server_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
```

- Avoid needless clones: Move owned loop variables into async tasks.
```rust
// Good: move ownership into the async task; no clone needed
join_set.spawn(async move {
    // use moved `server_name`
    (server_name, some_async().await)
});
```

- Write clear errors: Include the offending name and required pattern.
```rust
let err = anyhow::anyhow!(
    "invalid server name '{}': must match pattern ^[a-zA-Z0-9_-]+$",
    server_name
);
```

**DON’Ts**
- Don’t spawn invalid: Never launch clients and then validate inside the task.
```rust
// Bad: spawning first, validating later
join_set.spawn(async move {
    if !is_valid_mcp_server_name(&server_name) {
        return (server_name, Err(anyhow::anyhow!("invalid name")));
    }
    (server_name, Ok(connect().await?))
});
```

- Don’t use generic util: Avoid adding one-off helpers to a catch‑all `util.rs`.
```rust
// Bad: core/src/util.rs
pub fn is_valid_server_name(_: &str) -> bool { /* ... */ } // too generic, wrong place
```

- Don’t clone unnecessarily: Avoid `clone()` when you already own the value.
```rust
// Bad: unnecessary clone
let server_name_cloned = server_name.clone();
join_set.spawn(async move {
    (server_name_cloned, do_work().await)
});
```