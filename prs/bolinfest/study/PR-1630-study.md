**DOs**
- **Use `Command::cargo_bin` in tests**: Run the built binary directly instead of shelling out to `cargo run`.
```rust
use assert_cmd::prelude::*;
use std::process::Command;

let mut cmd = Command::cargo_bin("codex").unwrap(); // match the actual bin name
cmd.arg("exec").arg("--skip-git-repo-check");
```
- **Keep test-only crates in `dev-dependencies` (alpha‑sorted)**: Add only what you use; sort keys.
```toml
[dev-dependencies]
assert_cmd = "2"
predicates = "3.1.3"
tempfile = "3"
uuid = { version = "1", features = ["serde", "v4"] }
walkdir = "2.5.0"
wiremock = "0.6"
```
- **Write JSONL via a helper that flushes**: Centralize serialization, newline, write, and flush; log errors.
```rust
async fn write_json_line<T: serde::Serialize>(
    file: &mut tokio::fs::File,
    value: &T,
) -> std::io::Result<()> {
    let mut buf = serde_json::to_vec(value)?;
    buf.push(b'\n');
    file.write_all(&buf).await?;
    file.flush().await?;
    Ok(())
}

// usage
if let Err(e) = write_json_line(&mut file, &meta).await {
    warn!("Failed to write session meta: {e}");
}
```
- **Use UTC for timestamps**: Store times in UTC; convert for display elsewhere.
```rust
use time::OffsetDateTime;
let timestamp = OffsetDateTime::now_utc();
```
- **Propagate `cwd` when resuming and include Git info in meta**: Capture repo context in the first log line.
```rust
// resume signature and call
pub async fn resume(path: &Path, cwd: std::path::PathBuf) -> io::Result<(Self, SavedSession)> { ... }
let (rec, saved) = RolloutRecorder::resume(path, cwd.clone()).await?;

// meta with git
let git = collect_git_info(&cwd).await;
let meta = SessionMetaWithGit { meta, git };
write_json_line(&mut file, &meta).await?;
```
- **Prefer separate declarations for clarity**: Keep bindings simple and explicit.
```rust
let mut restored_items: Option<Vec<ResponseItem>> = None;
let mut recorder_opt: Option<RolloutRecorder> = None;
```
- **Inline variables with `format!`**: Use captured identifiers directly in braces.
```rust
let resume_override = format!("experimental_resume=\"{resume_path_str}\"");
```
- **Avoid blocking I/O on caller’s thread**: Use a bounded channel + background task for file writes.
```rust
let (tx, rx) = tokio::sync::mpsc::channel::<RolloutCmd>(256);
tokio::task::spawn(rollout_writer(file, rx, Some(meta), cwd));
```

**DON’Ts**
- **Don’t shell out to `cargo run` in tests**: Avoid spawning Cargo to reach your binary.
```rust
// avoid
let mut cmd = assert_cmd::Command::new("cargo");
cmd.arg("run").arg("-p").arg("codex-cli").arg("--"); // ✗
```
- **Don’t double‑flush after a helper that flushes**: One flush in the helper is sufficient.
```rust
write_json_line(&mut file, &meta).await?; // already flushes
// file.flush().await?; // ✗ redundant
```
- **Don’t use local time in logs**: Avoid timezone‑dependent timestamps.
```rust
// let timestamp = OffsetDateTime::now_local(); // ✗ prefer UTC
```
- **Don’t put test‑only crates in `[dependencies]` or leave them unsorted**: Keep them in `[dev-dependencies]`.
```toml
[dependencies]
# assert_cmd = "2"  # ✗ test-only; move to [dev-dependencies]
```
- **Don’t combine unrelated initializations into one binding**: Skip tuple destructuring for separate states.
```rust
// let (mut restored_items, mut recorder_opt) = (None, None); // ✗
```
- **Don’t perform blocking writes in async paths**: Avoid `std::fs` writes on async threads.
```rust
// std::fs::File::create(path)?.write_all(&buf)?; // ✗ use tokio + task
```
- **Don’t forget to pass `cwd` on resume**: Missing it drops Git context in metadata.
```rust
// RolloutRecorder::resume(path).await?; // ✗ missing cwd
```