**DOs**

- **Use Raw Strings for SSE/JSON**: Prefer `r#"... "#` when not formatting to avoid escaping braces and quotes.
```rust
let sse = r#"data: {"choices":[{"delta":{"content":"hi"}}]}

data: {"choices":[{"delta":{}}]}

data: [DONE]
"#;
```

- **Run Binaries With cargo_bin**: Use `assert_cmd` to execute the compiled CLI instead of spawning `cargo run`.
```rust
use assert_cmd::prelude::*;
use std::process::Command;

let mut cmd = Command::cargo_bin("codex-cli").unwrap();
cmd.arg("exec")
   .arg("--skip-git-repo-check")
   .arg("-c")
   .arg(format!(
       "model_providers.mock={{ name=\"mock\", base_url=\"{}/v1\", env_key=\"PATH\", wire_api=\"chat\" }}",
       server.uri()
   ))
   .arg("-c")
   .arg("model_provider=mock")
   .arg("-C")
   .arg(env!("CARGO_MANIFEST_DIR"))
   .arg("hello?")
   .env("CODEX_HOME", home.path())
   .env("OPENAI_API_KEY", "dummy")
   .env("OPENAI_BASE_URL", format!("{}/v1", server.uri()))
   .assert()
   .success();
```

- **Set Working Dir Via -C/--cd**: Ask the CLI to change directories rather than mutating the test process’s CWD.
```rust
let mut cmd = Command::cargo_bin("codex-cli").unwrap();
cmd.args(["exec", "-C", env!("CARGO_MANIFEST_DIR"), "hello?"]);
```

- **Terminate Streams With [DONE]**: Include the `[DONE]` sentinel in mock SSE streams; use empty `{}` deltas only as interim chunks.
```rust
let sse = r#"data: {"choices":[{"delta":{"content":"hi"}}]}

data: {"choices":[{"delta":{}}]}

data: [DONE]
"#;
```

**DON’Ts**

- **Don’t Over‑Escape JSON/SSE**: Avoid `concat!` with escaped quotes/braces when a raw string will do.
```rust
// Bad
let sse = concat!(
  "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
  "data: {\"choices\":[{\"delta\":{}}]}\n\n",
  "data: [DONE]\n\n",
);
```

- **Don’t Spawn cargo run Unless Necessary**: It’s slower and less direct than running the binary under test.
```rust
// Bad
let mut cmd = std::process::Command::new("cargo");
cmd.args(["run", "-p", "codex-cli", "--", "exec", "hello?"]);
```

- **Don’t Rely on current_dir When CLI Supports -C**: Prefer the CLI’s own directory flag for reproducibility and clarity.
```rust
// Bad
cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
```

- **Don’t Treat Empty `{}` as End‑Of‑Stream**: Without `[DONE]`, completion is ambiguous.
```rust
// Bad: missing [DONE]
let sse = r#"data: {"choices":[{"delta":{"content":"hi"}}]}

data: {"choices":[{"delta":{}}]}
"#;
```