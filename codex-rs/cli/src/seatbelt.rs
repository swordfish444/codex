use codex_core::exec::StdioPolicy;
use codex_core::exec::spawn_command_under_seatbelt;
use codex_core::protocol::SandboxPolicy;
use std::os::unix::process::ExitStatusExt;
use std::process;

pub async fn run_seatbelt(
    command: Vec<String>,
    sandbox_policy: SandboxPolicy,
) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().expect("failed to get cwd");
    let mut child =
        spawn_command_under_seatbelt(command, &sandbox_policy, cwd, StdioPolicy::Inherit).await?;
    let status = child.wait().await?;

    // Use ExitStatus to derive the exit code.
    if let Some(code) = status.code() {
        process::exit(code);
    } else if let Some(signal) = status.signal() {
        process::exit(128 + signal);
    } else {
        process::exit(1);
    }
}
