**DOs**

- **Extract Module (`git_info.rs`)**: Isolate Git logic in its own module and import from call sites.
```rust
// core/src/lib.rs
pub mod git_info;

// core/src/rollout.rs
use crate::git_info::{collect_git_info, GitInfo};
```

- **Order Exports Clearly**: Put the primary async API right after the struct so the module’s surface is obvious.
```rust
// core/src/git_info.rs
#[derive(Serialize, Deserialize, Clone)]
pub struct GitInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

pub async fn collect_git_info(cwd: &Path) -> Option<GitInfo> { /* ... */ }
```

- **Use Tokio + Timeout**: Use `tokio::process::Command` and `tokio::time::timeout` with a 5s cap.
```rust
const GIT_COMMAND_TIMEOUT: TokioDuration = TokioDuration::from_secs(5);

async fn run_git_command_with_timeout(args: &[&str], cwd: &Path) -> Option<std::process::Output> {
    timeout(
        GIT_COMMAND_TIMEOUT,
        Command::new("git").args(args).current_dir(cwd).output(),
    ).await.ok().and_then(Result::ok)
}
```

- **Parallelize Git Calls**: Run independent git commands concurrently.
```rust
let (commit, branch, url) = tokio::join!(
    run_git_command_with_timeout(&["rev-parse", "HEAD"], cwd),
    run_git_command_with_timeout(&["rev-parse", "--abbrev-ref", "HEAD"], cwd),
    run_git_command_with_timeout(&["remote", "get-url", "origin"], cwd),
);
```

- **Write Metadata In Writer Task**: Compute git info inside `rollout_writer` and write meta+git before processing messages to avoid startup stalls.
```rust
async fn rollout_writer(
    mut file: tokio::fs::File,
    mut rx: mpsc::Receiver<RolloutCmd>,
    mut meta: Option<SessionMeta>,
    cwd: std::path::PathBuf,
) {
    if let Some(session_meta) = meta.take() {
        let git = collect_git_info(&cwd).await;
        let payload = SessionMetaWithGit { meta: session_meta, git };
        if let Ok(line) = serde_json::to_string(&payload) {
            let _ = file.write_all(format!("{line}\n").as_bytes()).await;
            let _ = file.flush().await;
        }
    }
    while let Some(cmd) = rx.recv().await { /* ... */ }
}
```

- **Keep `SessionMeta` Immutable**: Don’t add a mutable/optional `git` field to it; use a wrapper with `flatten`.
```rust
#[derive(Serialize)]
pub struct SessionMeta { pub id: Uuid, pub timestamp: String, pub instructions: Option<String> }

#[derive(Serialize)]
struct SessionMetaWithGit {
    #[serde(flatten)]
    meta: SessionMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    git: Option<GitInfo>,
}
```

- **Pass `cwd` Explicitly Where Needed**: Require a `PathBuf` for writer/resume; avoid `Option` if it’s always available.
```rust
pub async fn resume(path: &Path, cwd: PathBuf) -> io::Result<(Self, SavedSession)> {
    // ...
    tokio::task::spawn(rollout_writer(tokio::fs::File::from_std(file), rx, None, cwd));
    Ok((Self { tx }, saved))
}
```

- **Use `Config.cwd` When Available**: In constructors where `Config` is present, use `config.cwd.clone()` instead of plumbing extra params.
```rust
let cwd = config.cwd.clone();
tokio::task::spawn(rollout_writer(tokio_file, rx, Some(meta), cwd));
```

- **Omit Empty Fields**: Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional metadata to keep rollouts tidy.
```rust
#[derive(Serialize)]
struct GitInfo { #[serde(skip_serializing_if = "Option::is_none")] repository_url: Option<String> /* ... */ }
```

- **Stabilize Tests With Threads (if needed)**: Prefer adding worker threads over disabling features.
```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn integration_git_info_unit_test() { /* ... */ }
```

- **Prefer Config Over Env Flags**: If you must toggle in tests, add an `experimental_*` config knob.
```rust
// Pseudocode
if config.experimental_collect_git_metadata.unwrap_or(true) {
    git = collect_git_info(&cwd).await;
}
```


**DON’Ts**

- **Don’t Block Startup**: Avoid awaiting `collect_git_info()` inside `RolloutRecorder::new()`. Compute it in the writer task instead.
```rust
// ❌ Anti-pattern
let git = collect_git_info(&config.cwd).await; // blocks startup
recorder.record_item(&with_git(meta, git)).await?;
```

- **Don’t Mutate `SessionMeta`**: Don’t add `git: Option<...>` directly to `SessionMeta`; keep it stable and wrap it.
```rust
// ❌ Anti-pattern
#[derive(Serialize)]
struct SessionMeta { /* ... */ git: Option<GitInfo> } // introduces mutability/optionality
```

- **Don’t Introduce New Env Flags**: Avoid `CODEX_DISABLE_GIT_INFO`-style switches; use config or runtime settings.
```rust
// ❌ Anti-pattern
if std::env::var("CODEX_DISABLE_GIT_INFO").is_ok() { /* ... */ }
```

- **Don’t Use `std::process::Command` In Async Paths**: It blocks; use Tokio’s async process API.
```rust
// ❌ Anti-pattern
std::process::Command::new("git").args(args).output().unwrap(); // blocking
```

- **Don’t Run Git Commands Serially**: Parallelize with `tokio::join!` instead of sequential awaits.
```rust
// ❌ Anti-pattern
let a = run_git(...).await;
let b = run_git(...).await; // slower, serial
```

- **Don’t Use `Option<PathBuf>` For Required `cwd`**: If `cwd` always exists, make it a required parameter.
```rust
// ❌ Anti-pattern
fn rollout_writer(..., cwd: Option<PathBuf>) { /* ... */ }
```

- **Don’t Leave Dead/Trailing Code**: Remove leftover blocks and unused helpers after refactors.
```rust
// ❌ Anti-pattern
// stray block or unused fn lingering at end of file
```

- **Don’t Treat Detached HEAD As A Branch**: Map `"HEAD"` to `None` to reflect detached state.
```rust
let branch = String::from_utf8(out.stdout).ok().map(|s| s.trim().to_string());
let branch = branch.filter(|b| b != "HEAD");
```

- **Don’t Overly Aggressive Timeouts**: Avoid too-short caps that flake on large repos; prefer ~5s with async.
```rust
// ✅ Use 5s cap as a reasonable default
const GIT_COMMAND_TIMEOUT: TokioDuration = TokioDuration::from_secs(5);
```