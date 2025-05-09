use codex_core::exec::StdioPolicy;
use codex_core::exec::spawn_command_under_seatbelt;
use codex_core::protocol::SandboxPolicy;

pub async fn run_seatbelt(
    command: Vec<String>,
    sandbox_policy: SandboxPolicy,
) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().expect("failed to get cwd");
    let child =
        spawn_command_under_seatbelt(command, &sandbox_policy, cwd, StdioPolicy::Inherit).await;
    let status = child
        .map_err(|e| anyhow::anyhow!("Failed to spawn command: {}", e))?
        .wait()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wait for command: {}", e))?;
    std::process::exit(status.code().unwrap_or(1));
}
