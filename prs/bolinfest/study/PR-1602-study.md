**PR #1602 Review Takeaways**

**DOs**
- **Investigate Before Parsing Changes:** Reproduce the Windows quoting issue, document root cause, and add a regression test before modifying CLI `-c key=value` parsing.
```rust
// Build a TOML‑safe override; quotes + backslashes escaped.
let path = r"C:\logs\sessions\a.jsonl";
let arg = format!("-c experimental_resume={:?}", path);
// -> -c experimental_resume="C:\\logs\\sessions\\a.jsonl"

// Optional normalization approach (test both behaviors):
let arg_norm = format!(r#"-c experimental_resume="{}""#, path.replace('\\', "/"));
```

- **Decide Resume vs New Early (Keep IDs Immutable):** Determine the session to use up front and pass it through; avoid mutating IDs later.
```rust
use uuid::Uuid;

async fn choose_session_id(resume_path: Option<&std::path::Path>) -> std::io::Result<Uuid> {
    if let Some(p) = resume_path {
        let (_rec, saved) = RolloutRecorder::resume(p).await?;
        Ok(saved.session_id)
    } else {
        Ok(Uuid::new_v4())
    }
}

// Later:
let session_id = choose_session_id(config.experimental_resume.as_deref()).await?;
```

- **Propagate I/O Errors From Background Writers:** Have the writer return `io::Result<()>`, store the `JoinHandle`, and surface failures (e.g., in tests or shutdown).
```rust
async fn rollout_writer(
    mut file: tokio::fs::File,
    mut rx: tokio::sync::mpsc::Receiver<RolloutCmd>,
    meta: Option<SessionMeta>,
) -> std::io::Result<()> {
    if let Some(m) = meta {
        file.write_all(serde_json::to_string(&m)?.as_bytes()).await?;
        file.write_all(b"\n").await?;
    }
    while let Some(cmd) = rx.recv().await {
        // ... perform writes with `?`
    }
    file.flush().await?;
    Ok(())
}

// Keep the handle so callers can `await` or inspect errors.
let handle: tokio::task::JoinHandle<std::io::Result<()>> =
    tokio::spawn(rollout_writer(file, rx, Some(meta)));
```

- **Use `Command::cargo_bin` In Tests:** Prefer compiled binary execution over `cargo run` for speed and determinism.
```rust
use assert_cmd::cargo::CommandCargoExt;
use std::process::Command;

let mut cmd = Command::cargo_bin("codex-cli").unwrap();
cmd.arg("exec")
   .arg("--skip-git-repo-check")
   .arg("-c")
   .arg(format!("experimental_resume={:?}", path));
cmd.assert().success();
```

- **Add Regression Tests For CLI Overrides:** Cover quoting/escaping on Windows paths and fallback behavior so future changes don’t regress.
```rust
#[test]
fn windows_path_is_preserved_in_override() {
    let win = r"C:\Users\me\sessions\run.jsonl";
    let arg = format!("-c experimental_resume={:?}", win);
    assert!(arg.contains(r#""C:\\Users\\me\\sessions\\run.jsonl""#));
}
```

- **Document New Config Clearly:** If adding `experimental_resume`, specify expected format (absolute path, JSONL) and precedence.
```rust
/// Experimental: absolute path to a `.jsonl` rollout to resume.
/// Example: `-c experimental_resume="/home/user/.codex/sessions/2025/07/19/run.jsonl"`
pub experimental_resume: Option<std::path::PathBuf>,
```


**DON’Ts**
- **Don’t Mutate `session_id` Mid‑Flow:** Avoid patterns that change identity after configuration.
```rust
// Bad: mutating the ID later in the loop.
let mut session_id = Uuid::new_v4();
// ...
if let Some(saved) = maybe_resume_id { session_id = saved; }

// Good: compute once, immutably (see DOs).
```

- **Don’t Ignore I/O Errors:** Avoid `let _ = ...` in the writer; it hides failures that make rollouts incomplete or corrupt.
```rust
// Bad:
let _ = file.write_all(json.as_bytes()).await;

// Good:
file.write_all(json.as_bytes()).await?;
```

- **Don’t Land Parsing Changes Without Tests:** Trimming quotes on parse failure can mask real bugs and create platform‑specific surprises.
```rust
// Bad: silently “fixes” input and hides root cause.
let trimmed = value_str.trim().trim_matches(|c| c == '"' || c == '\'');
// Good: reproduce, understand, then fix with a tested approach (see DOs).
```

- **Don’t Use `cargo run` In Integration Tests:** It’s slower, noisier, and more brittle than invoking the built binary.
```rust
// Bad:
let mut cmd = std::process::Command::new("cargo");
cmd.args(["run", "-p", "codex-cli", "--", "exec"]);

// Good: use Command::cargo_bin (see DOs).
```

- **Don’t Hand‑Roll Fragile `-c` Strings:** Avoid manual quoting that breaks on Windows; use `{:?}` or normalize.
```rust
// Bad: breaks when `path` contains backslashes.
let arg = format!("-c experimental_resume=\"{path}\"");

// Good:
let arg = format!("-c experimental_resume={:?}", path);
// or normalize:
let arg = format!(r#"-c experimental_resume="{}""#, path.replace('\\', "/"));
```