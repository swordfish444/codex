**DOs**
- Boldly pass overrides end-to-end: thread `base_instructions` through config → session → prompt.
```rust
// During ConfigureSession:
Op::ConfigureSession {
    // ...
    user_instructions,
    base_instructions: config.base_instructions.clone(),
    // ...
};

// When building the Prompt:
let prompt = Prompt {
    // ...
    user_instructions: sess.user_instructions.clone(),
    base_instructions_override: sess.base_instructions.clone(),
    // ...
};
```

- Prefer helper utilities in tests to reduce flakiness and boilerplate.
```rust
// Good: wait for a specific event, then for TaskComplete.
let EventMsg::SessionConfigured(SessionConfiguredEvent { session_id, .. }) =
    test_support::wait_for_event(&codex, |ev| matches!(ev, EventMsg::SessionConfigured(_))).await
else { unreachable!() };

test_support::wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;
```

- Capture full return values from functions that now return more items; ignore what you don’t need.
```rust
// Accept signature evolution safely:
let (codex, ..) = Codex::spawn(config, ctrl_c.clone()).await?;
// or explicitly:
let (codex, _init_id, _extra) = Codex::spawn(config, ctrl_c.clone()).await?;
```

- Keep imports tidy after refactors; remove unused imports and the stray blank line they leave behind.
```rust
// Good: no unused imports and no extra blank lines.
use tempfile::TempDir;
use wiremock::{Mock, MockServer, ResponseTemplate};
```

- Treat `CodexToolCallParam.base_instructions` as literal text (not a file path).
```rust
use codex_mcp_server::CodexToolCallParam;

let params = CodexToolCallParam {
    prompt: "How are you?".to_string(),
    base_instructions: Some("You are a helpful assistant.".to_string()),
    ..Default::default()
};
```

- Support file-based overrides via config for local workflows.
```toml
# config.toml
experimental-instructions-file = "/abs/path/to/instructions.md"
```
```rust
fn get_base_instructions(path: Option<&PathBuf>) -> Option<String> {
    let path = path.as_ref()?;
    std::fs::read_to_string(path).ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
```

- Compose final instructions with the correct precedence: base first, then user.
```rust
let base = self.base_instructions_override.as_deref().unwrap_or(BASE_INSTRUCTIONS);
let mut sections = vec![base];
if let Some(ref user) = self.user_instructions {
    sections.push(user);
}
let full = sections.join("\n\n");
```

- Write assertions that don’t move `Option` values unnecessarily.
```rust
let maybe_id = Some(session_id.to_string());
assert!(maybe_id.is_some());
assert_eq!(request_body.to_str().unwrap(), maybe_id.as_ref().unwrap());
```

- Verify overrides actually reach the wire in both APIs under test.
```rust
// responses API: body has top-level "instructions"
let body = request.body_json::<serde_json::Value>().unwrap();
assert!(body["instructions"].as_str().unwrap().contains("test instructions"));

// chat.completions API: first system message content
let body = request.body_json::<serde_json::Value>().unwrap();
let sys = body["messages"][0]["content"].as_str().unwrap();
assert!(sys.starts_with("You are a helpful assistant."));
```

- Derive `Default` for test-friendly params.
```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct CodexToolCallParam { /* ... */ }
```

**DON’Ts**
- Don’t leave stale imports or extra blank lines after removing an import.
```rust
// Bad:
use tokio::time::timeout;

            // <- orphaned blank line after removing usage
```

- Don’t assume `Codex::spawn` still returns exactly two values.
```rust
// Bad: breaks if a third value was added.
let (codex, init_id) = Codex::spawn(config, ctrl_c).await?;
```

- Don’t pass file paths via `base_instructions` in the MCP tool call.
```rust
// Bad: this field expects literal instructions text, not a path.
base_instructions: Some("/tmp/instructions.md".to_string())
```

- Don’t move `Option` values you still need later.
```rust
// Bad:
assert_eq!(header, maybe_id.unwrap()); // moved, cannot use maybe_id again
```

- Don’t duplicate ad-hoc event polling loops in tests; use the shared helper.
```rust
// Bad: open-coded loop with timeouts and pattern matching everywhere.
loop {
    // ...
    if matches!(ev.msg, EventMsg::TaskComplete(_)) { break; }
}
```

- Don’t ignore the intended precedence: base instructions (built-in or override) must come before user instructions.
```rust
// Bad: user first, base second (reverses meaning).
let mut sections = vec![user, base];
```

- Don’t treat `base_instructions` and `user_instructions` as interchangeable; they serve different purposes.
```rust
// Bad: stuffing user content into base overrides changes global behavior.
config.base_instructions = config.user_instructions.clone();
```

- Don’t forget to trim and validate file-based overrides.
```rust
// Bad: accepts empty files and trailing whitespace verbatim.
std::fs::read_to_string(path).ok()
```