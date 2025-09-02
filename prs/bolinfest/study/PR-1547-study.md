**PR #1547 Review Takeaways**

**DOs**
- Bold Assertions: prefer whole-structure equality over piecemeal checks.
- Network Gating: skip net-dependent tests when sandboxed.
- Test Docstrings: explain what each test verifies.
- Helpers: factor repeated setup into small helpers.
- Strong Lookups: use `find(...).unwrap()` then assert the exact value.
- Config Over Env: set retry counts on `Config`, not via env vars.
- Quiet Tests: avoid printing; assert on outputs.
- Formatting/Lints: run `just fmt` and fix clippy warnings.
- Inline Formatting: use `format!` with inline `{}`.
- Pretty Diffs: use `pretty_assertions::assert_eq`.

```rust
// DO: assert the entire payload (messages)
/// Validates that we send the exact Chat Completions messages payload.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn assembles_messages_correctly() {
    if std::env::var(crate::exec::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        return; // network gated
    }

    // ... set up `prompt`, `config`, `client`, `provider`, and invoke API ...

    let body: serde_json::Value = captured_body.lock().unwrap().take().unwrap();
    let messages = body.get("messages").unwrap();

    let expected = serde_json::json!([
        {"role":"system","content":prompt.get_full_instructions(&config.model)},
        {"role":"user","content":"hi"},
        {"role":"assistant","content":"ok"},
        {"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"foo","arguments":"{}"}}]},
        {"role":"tool","tool_call_id":"c1","content":"out"}
    ]);

    pretty_assertions::assert_eq!(messages, &expected);
}
```

```rust
// DO: gate tests that touch the network
if std::env::var(crate::exec::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
    return;
}
```

```rust
// DO: add a brief docstring for each test
/// Retries once on 500 and then streams a Completed event.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retries_once_on_server_error() { /* ... */ }
```

```rust
// DO: helper functions to dedupe setup
fn sse_completed(id: &str) -> String {
    format!(
        "event: response.completed\n\
         data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{id}\",\"output\":[]}}}}\n\n\n"
    )
}

fn create_test_client(server: &wiremock::MockServer, max_retries: u64) -> ModelClient {
    let provider = ModelProviderInfo {
        name: "openai".into(),
        base_url: format!("{}/v1", server.uri()),
        env_key: Some("PATH".into()),
        env_key_instructions: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
    };
    let mut cfg = default_config(provider.clone(), max_retries);
    cfg.openai_request_max_retries = max_retries;
    ModelClient::new(cfg, provider, ReasoningEffortConfig::None, ReasoningSummaryConfig::None)
}
```

```rust
// DO: strong lookup + full-value assert for tools
let tools = create_tools_json_for_responses_api(&prompt, "gpt-4")?;
let dummy = tools
    .iter()
    .find(|t| t.get("name") == Some(&"srv.dummy".into()))
    .unwrap();
let expected_dummy = mcp_tool_to_openai_tool("srv.dummy".into(), prompt.extra_tools.remove("srv.dummy").unwrap());
pretty_assertions::assert_eq!(dummy, &expected_dummy);
```

```rust
// DO: configure retries via Config, not env vars
let mut cfg = default_config(provider.clone(), 0);
cfg.openai_request_max_retries = 1; // instead of unsafe set_var
```

```rust
// DO: keep tests quiet; assert on content
let stdout = String::from_utf8_lossy(&output.stdout);
let hi_lines = stdout.lines().filter(|line| line.trim() == "hi").count();
assert_eq!(hi_lines, 1, "Expected exactly one line with 'hi'");
```

```rust
// DO: assert the entire tools list (documents wire payload)
let tools = create_tools_json_for_chat_completions_api(&prompt, "gpt-4")?;
let expected = serde_json::json!([
  {"type":"function","function":{"name":"shell","description":"...","parameters":{"type":"object","properties":{}}}},
  {"type":"function","function":{"name":"srv.dummy","description":"dummy","parameters":{"type":"object"}}}
]);
pretty_assertions::assert_eq!(tools, expected.as_array().unwrap());
```

---

**DON’Ts**
- Partial Field Checks: don’t assert a few fields when you can compare the full structure.
- Unsafe Env Mutation: don’t use `unsafe { std::env::set_var(...) }` in tests to tune retries.
- Debug Noise: don’t `println!` test internals; rely on assertions.
- Duplicated Setup: don’t inline identical server/client boilerplate in every test.
- Ungated Net Tests: don’t run network tests when the sandbox disables networking.

```rust
// DON'T: piecewise asserts that miss regressions
assert_eq!(messages[1]["role"], "user");
assert_eq!(messages[2]["role"], "assistant");
// vs. DO: pretty_assertions::assert_eq!(messages, &expected);
```

```rust
// DON'T: mutate env for retries (unsafe + racy)
// unsafe { std::env::set_var("OPENAI_REQUEST_MAX_RETRIES", "1"); }

// DO instead:
// cfg.openai_request_max_retries = 1;
```

```rust
// DON'T: print in tests
// println!("Status: {status}");
// println!("Stdout: {stdout}");
```