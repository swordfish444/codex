**DOs**
- Put test-only crates under `[dev-dependencies]`: keeps runtime clean and avoids unnecessary transitive deps.
```toml
# core/Cargo.toml (right)
[dependencies]
uuid = { version = "1", features = ["serde", "v4"] }
# … other runtime deps …

[dev-dependencies]
walkdir = "2.5.0"  # used only in tests
```

- Prefer `Command::cargo_bin` in tests: runs the built binary directly instead of spawning `cargo run`.
```rust
use assert_cmd::prelude::*;
use assert_cmd::Command;
use tempfile::TempDir;

let home = TempDir::new().unwrap();

Command::cargo_bin("codex-cli")
    .unwrap()
    .args([
        "exec",
        "--skip-git-repo-check",
        "-C",
        env!("CARGO_MANIFEST_DIR"),
        "echo integration-test-marker",
    ])
    .env("CODEX_HOME", home.path())
    .env("OPENAI_API_KEY", "dummy")
    .env("OPENAI_BASE_URL", "http://unused.local")
    .assert()
    .success();
```

**DON’Ts**
- Don’t add test-only crates to `[dependencies]`: this bloats the runtime dependency graph.
```toml
# core/Cargo.toml (wrong)
[dependencies]
walkdir = "2.5.0"  # ❌ test-only; should be under [dev-dependencies]
```

- Don’t shell out to `cargo run` from tests unless there’s a compelling reason.
```rust
// ❌ Avoid this in tests
use assert_cmd::Command as AssertCommand;

let mut cmd = AssertCommand::new("cargo");
cmd.arg("run")
   .arg("-p").arg("codex-cli")
   .arg("--")
   .arg("exec")
   .arg("--skip-git-repo-check")
   .arg("-C").arg(env!("CARGO_MANIFEST_DIR"))
   .arg("echo integration-test-marker");

// Prefer Command::cargo_bin(...) instead (see DOs).
```