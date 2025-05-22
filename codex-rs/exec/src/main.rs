//! Entry-point for the `codex-exec` binary.
//!
//! When this CLI is invoked normally, it parses the standard `codex-exec` CLI
//! options and launches the non-interactive Codex agent. However, if it is
//! invoked with arg0 as `codex-linux-sandbox`, we instead treat the invocation
//! as a request to run the logic for the standalone `codex-linux-sandbox`
//! executable (i.e., parse any -s args and then run a *sandboxed* command under
//! Landlock + seccomp.
//!
//! This allows us to ship a completely separate set of functionality as part
//! of the `codex-exec` binary.
use clap::Parser;
use codex_exec::Cli;
use codex_exec::run_main;
use std::path::Path;

// No #[tokio::main]! If arg0 is `codex-linux-sandbox`, we delegate to
// `codex_linux_sandbox::run_main()` and do not want to start the Tokio runtime.
fn main() -> anyhow::Result<()> {
    // Determine if we were invoked via the special alias.
    let argv0 = std::env::args().next().unwrap_or_default();
    let exe_name = Path::new(&argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if exe_name == "codex-linux-sandbox" {
        codex_linux_sandbox::run_main()
    }

    // Regular `codex-exec` invocation – parse the normal CLI.
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let cli = Cli::parse();
        run_main(cli).await?;
        Ok(())
    })
}
