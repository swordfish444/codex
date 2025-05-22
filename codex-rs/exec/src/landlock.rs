//! Minimal Landlock + seccomp helper that can be reused by multiple crates.
//!
//! The implementation is copied from the equivalent helper in the `codex-cli`
//! crate so we can invoke it from the `codex-exec` binary when it is executed
//! through the `codex-linux-sandbox` symlink/alias.

#[cfg(not(target_os = "linux"))]
pub fn run_landlock(_command: Vec<String>, _config: &codex_core::config::Config) -> anyhow::Result<()> {
    anyhow::bail!("Landlock sandboxing is only supported on Linux.");
}

#[cfg(target_os = "linux")]
pub fn run_landlock(command: Vec<String>, config: &codex_core::config::Config) -> anyhow::Result<()> {
    use codex_core::exec::spawn_child_sync;
    use codex_core::exec::StdioPolicy;
    use codex_core::exec_env;
    use codex_core::exec_linux::apply_sandbox_policy_to_current_thread;
    use std::process::ExitStatus;

    // Borrowing the helper from the CLI implementation: most error handling is
    // kept verbatim.

    if command.is_empty() {
        anyhow::bail!("command args are empty");
    }

    // Build the environment to pass to the child process based on the config.
    let env = exec_env::create_env(&config.shell_environment_policy);
    let sandbox_policy = config.sandbox_policy.clone();

    // Spawn a dedicated thread so the sandbox only affects the child process
    // and does not leak into the current one.
    let handle = std::thread::spawn(move || -> anyhow::Result<ExitStatus> {
        let cwd = std::env::current_dir()?;

        // Apply Landlock + seccomp restrictions to the *current thread* so they
        // are inherited by the forthcoming child process.
        apply_sandbox_policy_to_current_thread(&sandbox_policy, &cwd)?;

        let mut child = spawn_child_sync(command, cwd, &sandbox_policy, StdioPolicy::Inherit, env)?;
        let status = child.wait()?;
        Ok(status)
    });

    let status = handle
        .join()
        .map_err(|e| anyhow::anyhow!("Failed to join thread: {e:?}"))??;

    crate::exit_status::handle_exit_status(status);

    Ok(())
}
