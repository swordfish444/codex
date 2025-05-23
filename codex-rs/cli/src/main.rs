use clap::Parser;
use codex_cli::LandlockCommand;
use codex_cli::SeatbeltCommand;
use codex_cli::create_sandbox_policy;
use codex_cli::proto;
use codex_cli::seatbelt;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::exec_env::create_env;
use codex_exec::Cli as ExecCli;
use codex_tui::Cli as TuiCli;
use std::path::Path;
use std::path::PathBuf;

use crate::proto::ProtoCli;

/// Codex CLI
///
/// If no subcommand is specified, options will be forwarded to the interactive CLI.
#[derive(Debug, Parser)]
#[clap(
    author,
    version,
    // If a sub‑command is given, ignore requirements of the default args.
    subcommand_negates_reqs = true
)]
struct MultitoolCli {
    #[clap(flatten)]
    interactive: TuiCli,

    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    /// Run Codex non-interactively.
    #[clap(visible_alias = "e")]
    Exec(ExecCli),

    /// Experimental: run Codex as an MCP server.
    Mcp,

    /// Run the Protocol stream via stdin/stdout
    #[clap(visible_alias = "p")]
    Proto(ProtoCli),

    /// Internal debugging commands.
    Debug(DebugArgs),
}

#[derive(Debug, Parser)]
struct DebugArgs {
    #[command(subcommand)]
    cmd: DebugCommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugCommand {
    /// Run a command under Seatbelt (macOS only).
    Seatbelt(SeatbeltCommand),

    /// Run a command under Landlock+seccomp (Linux only).
    Landlock(LandlockCommand),
}

#[derive(Debug, Parser)]
struct ReplProto {}

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

    // Regular `codex` invocation – parse the normal CLI.
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let codex_linux_sandbox_exe: Option<PathBuf> = if cfg!(target_os = "linux") {
            std::env::current_exe().ok()
        } else {
            None
        };

        cli_main(codex_linux_sandbox_exe).await?;
        Ok(())
    })
}

async fn cli_main(codex_linux_sandbox_exe: Option<PathBuf>) -> anyhow::Result<()> {
    let cli = MultitoolCli::parse();

    match cli.subcommand {
        None => {
            codex_tui::run_main(cli.interactive, codex_linux_sandbox_exe)?;
        }
        Some(Subcommand::Exec(exec_cli)) => {
            codex_exec::run_main(exec_cli, codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::Mcp) => {
            codex_mcp_server::run_main(codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::Proto(proto_cli)) => {
            proto::run_main(proto_cli).await?;
        }
        Some(Subcommand::Debug(debug_args)) => match debug_args.cmd {
            DebugCommand::Seatbelt(SeatbeltCommand {
                command,
                sandbox,
                full_auto,
            }) => {
                let sandbox_policy = create_sandbox_policy(full_auto, sandbox);
                let config = Config::load_with_overrides(ConfigOverrides {
                    sandbox_policy: Some(sandbox_policy),
                    ..Default::default()
                })?;
                seatbelt::run_seatbelt(command, &config).await?;
            }
            DebugCommand::Landlock(LandlockCommand {
                command,
                sandbox,
                full_auto,
            }) => {
                let sandbox_policy = create_sandbox_policy(full_auto, sandbox);
                let cwd = std::env::current_dir()?;
                let config = Config::load_with_overrides(ConfigOverrides {
                    sandbox_policy: Some(sandbox_policy),
                    ..Default::default()
                })?;
                let sandbox_command_args = codex_core::exec::create_linux_sandbox_command_args(
                    command,
                    &config.sandbox_policy,
                    &cwd,
                );

                let codex_linux_sandbox_exe = codex_linux_sandbox_exe
                    .ok_or(anyhow::anyhow!("codex-linux-sandbox executable not found"))?;
                let env = create_env(&config.shell_environment_policy);
                codex_core::exec::spawn_command_under_linux_sandbox(
                    codex_linux_sandbox_exe,
                    sandbox_command_args,
                    &config.sandbox_policy,
                    cwd,
                    codex_core::exec::StdioPolicy::Inherit,
                    env,
                )
                .await?;
            }
        },
    }

    Ok(())
}
