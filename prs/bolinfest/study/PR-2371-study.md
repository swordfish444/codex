**DOs**
- Centralize tool-type remapping: keep “web_search” → “web_search_preview” logic inside the tool JSON builder, not at call sites.
```rust
fn create_tools_json_for_responses_api(
    tools: &[OpenAiTool],
    auth_mode: Option<AuthMode>,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut out = build_tools_json(tools)?;
    if auth_mode == Some(AuthMode::ChatGPT) {
        for t in &mut out {
            if t.get("type").and_then(|v| v.as_str()) == Some("web_search") {
                t["type"] = serde_json::Value::String("web_search_preview".to_string());
            }
        }
    }
    Ok(out)
}
```

- Match SSE events explicitly: add a dedicated arm for “response.output_item.added” and siblings; don’t bury logic inside a grouped catch‑all.
```rust
match event.kind.as_str() {
    "response.output_item.added" => {
        if let Some(item) = event.item.as_ref() {
            if item.get("type").and_then(|v| v.as_str()) == Some("web_search_call") {
                let call_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let query = item.get("query").and_then(|v| v.as_str()).map(|s| s.to_string());
                let _ = tx_event.send(Ok(ResponseEvent::WebSearchCallBegin { call_id, query })).await;
            }
        }
    }
    "response.output_item.done" => {
        if let Some(item) = event.item.as_ref() {
            if item.get("type").and_then(|v| v.as_str()) == Some("web_search_call") {
                let call_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _ = tx_event.send(Ok(ResponseEvent::WebSearchCallEnd { call_id })).await;
            }
        }
    }
    other => debug!(kind=%other, "sse event"),
}
```

- Pair begin/end events: introduce a matching “end” for every “begin”, propagate `call_id` (and the real `query`) through to the UI.
```rust
#[derive(Debug)]
pub enum ResponseEvent {
    WebSearchCallBegin { call_id: String, query: Option<String> },
    WebSearchCallEnd { call_id: String },
}
```
```rust
match msg {
    EventMsg::WebSearchBegin(ev) => self.add_to_history(history_cell::new_web_search_call(ev.query)),
    EventMsg::WebSearchEnd(_) => self.flush_answer_stream_with_separator(),
    _ => {}
}
```

- Keep config borrowing tight: avoid unnecessary clones of config fields; clone only when lifetimes demand it.
```rust
// Good
let shell_environment_policy = cfg.shell_environment_policy.into();

// Only clone when needed
let chatgpt_base_url = cfg.chatgpt_base_url.clone().unwrap_or_else(|| "...".to_string());
```

- Use a builder/struct for many config flags: replace “boolean soup” with named fields for clarity and test readability.
```rust
struct ToolsConfigBuilder {
    sandbox_policy: SandboxPolicy,
    plan_tool: bool,
    apply_patch_tool_type: Option<ApplyPatchToolType>,
    web_search_request: bool,
    streamable_shell: bool,
}

impl ToolsConfigBuilder {
    fn build(self) -> ToolsConfig { /* ... */ }
}

let tools_cfg = ToolsConfigBuilder {
    sandbox_policy: SandboxPolicy::ReadOnly,
    plan_tool: true,
    apply_patch_tool_type: None,
    web_search_request: true,
    streamable_shell: false,
}.build();
```

- Keep override parsing consistent across bins: use the same API in TUI and exec; avoid ad‑hoc rewrapping.
```rust
let cli_kv_overrides = cli.config_overrides.parse_overrides()?;
```

- Keep JSON schema test types stable: only switch to `String` when the schema struct requires it; otherwise keep `&str` literals to avoid churn.
```rust
// If the schema is Vec<&'static str>
required: vec!["string_property", "number_property"]

// If the schema is Vec<String>
required: vec!["string_property".to_string(), "number_property".to_string()]
```

- Preserve helpful logging: keep a default debug branch to surface unexpected SSE kinds.
```rust
other => debug!(kind=%other, "sse event"),
```

**DON’Ts**
- Don’t scatter feature-specific remapping at call sites.
```rust
// Don’t do this inside request assembly
if auth_mode == Some(AuthMode::ChatGPT) {
    for tool in &mut tools_json {
        if tool["type"] == "web_search" {
            tool["type"] = "web_search_preview".into();
        }
    }
}
```

- Don’t hide web_search detection inside a generic “ignored events” block; give it a clear match arm.
```rust
// Avoid: nested if inside a grouped branch
| "response.in_progress" | "response.output_item.added" | "response.output_text.done" => {
    if event.kind == "response.output_item.added" { /* ... */ }
}
```

- Don’t emit only “begin” without a matching “end”; the UI needs completion signals.
```rust
// Incomplete
EventMsg::WebSearchBegin(/* ... */);
// Missing: EventMsg::WebSearchEnd
```

- Don’t default `query` to placeholders when the SSE payload can provide it; plumb the real value.
```rust
// Avoid
let q = query.unwrap_or_else(|| "Searching Web...".to_string());
```

- Don’t introduce `.clone()` where a borrow or move suffices; it adds allocations and noise.
```rust
// Avoid
let shell_environment_policy = cfg.shell_environment_policy.clone().into();
```

- Don’t expand positional boolean parameters; prefer a builder with named fields.
```rust
// Avoid
ToolsConfig::new(policy, /*plan*/ true, /*apply_patch*/ false, /*web_search*/ true, /*stream*/ false);
```

- Don’t claim backward compatibility for new fields unless there is a real migration; avoid speculative aliases.
```rust
// Avoid adding aliases “just in case”
#[serde(alias = "web_search_request")]
pub web_search: Option<bool>,
```

- Don’t change override parsing in one crate without aligning others.
```rust
// Avoid custom rewraps in TUI only
let raw = cli.config_overrides.raw_overrides.clone();
let cli_kv_overrides = codex_common::CliConfigOverrides { raw_overrides: raw }.parse_overrides()?;
```

- Don’t silence unknown SSE kinds; retain a debug log for troubleshooting.
```rust
// Avoid
_ => {}
```