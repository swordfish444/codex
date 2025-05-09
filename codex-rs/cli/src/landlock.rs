//! `debug landlock` implementation for the Codex CLI.
//!
//! On Linux the command is executed inside a Landlock + seccomp sandbox by
//! calling the low-level `exec_linux` helper from `codex_core::linux`.

use codex_core::exec::StdioPolicy;
use codex_core::linux::spawn_command_under_landlock;
use codex_core::protocol::SandboxPolicy;
use std::os::unix::process::ExitStatusExt;
use std::process;
use std::process::ExitStatus;

/// Execute `command` in a Linux sandbox (Landlock + seccomp) the way Codex
/// would.
pub fn run_landlock(command: Vec<String>, sandbox_policy: SandboxPolicy) -> anyhow::Result<()> {
    if command.is_empty() {
        anyhow::bail!("command args are empty");
    }

    // Spawn a new thread and apply the sandbox policies there.
    let handle = std::thread::spawn(move || -> anyhow::Result<ExitStatus> {
        let cwd = std::env::current_dir()?;
        let mut child =
            spawn_command_under_landlock(command, &sandbox_policy, cwd, StdioPolicy::Inherit)
                .await?;
        let status = child.wait().await?;
        Ok(status)
    });
    let status = handle
        .join()
        .map_err(|e| anyhow::anyhow!("Failed to join thread: {e:?}"))??;

    // Use ExitStatus to derive the exit code.
    if let Some(code) = status.code() {
        process::exit(code);
    } else if let Some(signal) = status.signal() {
        process::exit(128 + signal);
    } else {
        process::exit(1);
    }
}
