use crate::protocol::SandboxPolicy;
use crate::spawn::StdioPolicy;
use crate::spawn::spawn_child_async;
use serde_json;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tokio::process::Child;

const WINDOWS_SANDBOX_ARG1: &str = "--codex-run-as-windows-sandbox";

pub async fn spawn_command_under_windows_sandbox<P>(
    codex_windows_sandbox_exe: P,
    command: Vec<String>,
    command_cwd: PathBuf,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    stdio_policy: StdioPolicy,
    env: HashMap<String, String>,
) -> std::io::Result<Child>
where
    P: AsRef<Path>,
{
    let mut args = Vec::new();
    args.push(WINDOWS_SANDBOX_ARG1.to_string());
    args.push(sandbox_policy_cwd.to_string_lossy().to_string());
    let policy_json = serde_json::to_string(sandbox_policy).map_err(std::io::Error::other)?;
    args.push(policy_json);
    args.push("--".to_string());
    args.extend(command);

    spawn_child_async(
        codex_windows_sandbox_exe.as_ref().to_path_buf(),
        args,
        None,
        command_cwd,
        sandbox_policy,
        stdio_policy,
        env,
    )
    .await
}
