**DOs**
- **Validate With Stdlib, Not Regex:** Use a tiny helper that matches /^[a-zA-Z0-9_-]+$/ with ASCII checks.
```rust
fn is_valid_mcp_server_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
```

- **Validate Before Spawning Tasks:** Pre‑filter invalid servers, record errors, and skip spawning.
```rust
let mut join_set = JoinSet::new();
let mut errors = ClientStartErrors::new();

for (server_name, cfg) in mcp_servers {
    if !is_valid_mcp_server_name(&server_name) {
        let err = anyhow::anyhow!(
            "invalid server name '{}': must match pattern ^[a-zA-Z0-9_-]+$",
            server_name
        );
        errors.insert(server_name, err);
        continue;
    }

    join_set.spawn(async move {
        let McpServerConfig { command, args, env } = cfg;
        let client_res = McpClient::new_stdio_client(command, args, env).await;
        (server_name, client_res)
    });
}
```

- **Initialize Error Collection Early:** Create the error map before any async work to capture pre‑spawn failures.
```rust
let mut errors = ClientStartErrors::new(); // before the for-loop
```

- **Emit Clear, Actionable Errors:** Include the bad name and the exact allowed pattern; inline variables in `format!`.
```rust
let msg = format!(
    "invalid server name '{}': must match pattern ^[a-zA-Z0-9_-]+$",
    server_name
);
let err = anyhow::anyhow!(msg);
```

- **Keep Validation Single‑Sourced:** Reuse the same helper everywhere you need this check.
```rust
if !is_valid_mcp_server_name(name) { /* handle error */ }
```

**DON’Ts**
- **Don’t Add Unneeded Dependencies:** Skip `regex`/`regex-lite` (and related helpers) for simple ASCII checks.
```toml
# Cargo.toml — avoid adding a regex dependency just for this
[dependencies]
# regex-lite = "0.1"   # ← Don’t add for this use case
```

- **Don’t Spawn For Invalid Names:** Never launch a client task if validation fails.
```rust
// BAD: spawns even when name is invalid
join_set.spawn(async move { /* ... */ }); // without pre-check

// GOOD: guarded by is_valid_mcp_server_name (see DOs)
```

- **Don’t Allow Empty Or Punctuated Names:** Reject empty strings and names with spaces or symbols.
```rust
assert!(is_valid_mcp_server_name("abc-123"));
assert!(is_valid_mcp_server_name("abc_123"));
assert!(!is_valid_mcp_server_name(""));        // empty
assert!(!is_valid_mcp_server_name("abc 123")); // space
assert!(!is_valid_mcp_server_name("abc$123")); // symbol
```

- **Don’t Emit Vague Errors:** Avoid messages that fail to guide users.
```rust
// BAD
anyhow::bail!("invalid name");

// GOOD
anyhow::bail!(
    "invalid server name '{}': must match pattern ^[a-zA-Z0-9_-]+$",
    server_name
);
```

- **Don’t Duplicate Validation Logic:** Centralize the rule in one helper to prevent drift.
```rust
// BAD: multiple ad-hoc checks scattered around
// GOOD: call is_valid_mcp_server_name(...) everywhere
```