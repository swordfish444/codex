use clap::Parser;

/// Entry-point for the `codex-exec` binary.
///
/// When invoked normally it parses the standard `codex-exec` CLI options and
/// launches the non-interactive Codex agent.  However, if the executable name
/// is (or ends with) `codex-linux-sandbox` we instead treat the invocation as
/// a request to run a *sandboxed* command under Landlock + seccomp.  This
/// allows us to create a lightweight symlink alias instead of shipping a
/// separate binary — mirroring how macOS uses `/usr/bin/sandbox-exec`.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::path::Path;

    // Determine if we were invoked via the special alias.
    let argv0 = std::env::args().next().unwrap_or_default();
    let exe_name = Path::new(&argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if exe_name == "codex-linux-sandbox" || exe_name.ends_with("codex-linux-sandbox") {
        // Forward *all* remaining CLI args as the command to run in the sandbox.
        let cmd: Vec<String> = std::env::args().skip(1).collect();

        // Load default configuration (no overrides).  Users can influence the
        // sandbox behaviour via their standard ~/.codex/config.toml settings.
        let config = codex_core::config::Config::load_with_overrides(Default::default())?;

        // Execute the command under Landlock.  This call never returns on
        // success – it exits the process with the child’s status.
        codex_exec::landlock::run_landlock(cmd, &config)?;

        // The above helper either `exit`s (on success) or returns an error.
        unreachable!("run_landlock should not return on success");
    }

    // Regular `codex-exec` invocation – parse the normal CLI.
    use codex_exec::Cli;
    use codex_exec::run_main;

    let cli = Cli::parse();
    run_main(cli).await?;

    Ok(())
}
