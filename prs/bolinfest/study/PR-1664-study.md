**DOs**
- **Thread `cwd` Explicitly**: Accept and pass `Option<PathBuf>` through layers to avoid ambient state.
```rust
use std::path::PathBuf;

pub async fn apply_diff(diff: &str, cwd: Option<PathBuf>) -> anyhow::Result<()> {
    // ...
    Ok(())
}
```
- **Scope Git Commands**: Set `current_dir` on the command when `cwd` is provided.
```rust
let mut cmd = tokio::process::Command::new("git");
if let Some(dir) = cwd { cmd.current_dir(dir); }
let _ = cmd.args(["rev-parse", "--show-toplevel"]).output().await?;
```
- **Keep CLI Behavior Stable**: Pass `None` at the CLI boundary so existing usage is unchanged.
```rust
match subcommand {
    Some(Subcommand::Apply(mut apply_cli)) => {
        prepend_config_flags(&mut apply_cli.config_overrides, cli.config_overrides);
        run_apply_command(apply_cli, None).await?;
    }
    _ => {}
}
```
- **Write Hermetic Tests**: Avoid global CWD; pass the repo path directly to the function under test.
```rust
let repo_path = fixture.repo_path();
apply_diff_from_task(task_response, Some(repo_path.to_path_buf())).await?;
```
- **Fail Fast With Clear Errors**: Validate required task fields and bail with precise messages.
```rust
let turn = task.current_diff_task_turn
    .ok_or_else(|| anyhow::anyhow!("No diff turn found"))?;
if output_diff.is_none() {
    anyhow::bail!("No PR output item found");
}
// Example with a variable:
anyhow::bail!("Invalid repo: {}", repo.display());
```
- **Use Async-Friendly Processes**: Prefer `tokio::process::Command` in async code to avoid blocking.
```rust
use tokio::process::Command;

let _ = Command::new("git").args(["status"]).output().await?;
```

**DON’Ts**
- **Don’t Mutate Global CWD**: Avoid `std::env::set_current_dir` and ad-hoc guards; they’re flaky in async/concurrent tests.
```rust
let original = std::env::current_dir()?;                 // ❌
std::env::set_current_dir(&repo_path)?;                  // ❌
struct DirGuard(std::path::PathBuf);                     // ❌
impl Drop for DirGuard {
    fn drop(&mut self) { let _ = std::env::set_current_dir(&self.0); }
}
let _guard = DirGuard(original);
```
- **Don’t Rely On Ambient State**: Library functions shouldn’t assume the process CWD.
```rust
async fn apply_diff(diff: &str) -> anyhow::Result<()> {  // ❌ no cwd param
    tokio::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output().await?;
    Ok(())
}
```
- **Don’t Share Mutable Globals In Tests**: No shared state that can race across async tests.
```rust
static mut TEST_REPO: Option<std::path::PathBuf> = None; // ❌
```
- **Don’t Swallow Missing-Data Errors**: Never turn structural problems into silent successes.
```rust
if output_diff.is_none() { return Ok(()); }              // ❌ hides failure
```
- **Don’t Break CLI Semantics**: Refactors shouldn’t require callers to supply a path unnecessarily.
```rust
run_apply_command(apply_cli, Some(std::env::current_dir()?)).await?; // ❌ forces callers
```
- **Don’t Block In Async Contexts**: Avoid `std::process::Command` in async paths.
```rust
std::process::Command::new("git").output()?;             // ❌ blocks thread
```