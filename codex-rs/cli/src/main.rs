use clap::Parser;
use codex_cli::LandlockCommand;
use codex_cli::SeatbeltCommand;
use codex_cli::proto;
use codex_common::CliConfigOverrides;
use codex_exec::Cli as ExecCli;
use codex_tui::Cli as TuiCli;
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
    pub config_overrides: CliConfigOverrides,

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
    codex_linux_sandbox::run_with_sandbox(|codex_linux_sandbox_exe| async move {
        cli_main(codex_linux_sandbox_exe).await?;
        Ok(())
    })
}

async fn cli_main(codex_linux_sandbox_exe: Option<PathBuf>) -> anyhow::Result<()> {
    let cli = MultitoolCli::parse();

    match cli.subcommand {
        None => {
            let mut interactive = cli.interactive;
            // Prepend root-level overrides so they have lower precedence than
            // CLI-specific ones specified after the subcommand (if any).
            interactive
                .config_overrides
                .raw_overrides
                .splice(0..0, cli.config_overrides.raw_overrides.into_iter());

            codex_tui::run_main(interactive, codex_linux_sandbox_exe)?;
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            exec_cli
                .config_overrides
                .raw_overrides
                .splice(0..0, cli.config_overrides.raw_overrides.into_iter());

            codex_exec::run_main(exec_cli, codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::Mcp) => {
            codex_mcp_server::run_main(codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::Proto(mut proto_cli)) => {
            proto_cli
                .config_overrides
                .raw_overrides
                .splice(0..0, cli.config_overrides.raw_overrides.into_iter());
            proto::run_main(proto_cli).await?;
        }
        Some(Subcommand::Debug(debug_args)) => match debug_args.cmd {
            DebugCommand::Seatbelt(mut seatbelt_command) => {
                seatbelt_command
                    .config_overrides
                    .raw_overrides
                    .splice(0..0, cli.config_overrides.raw_overrides.into_iter());
                codex_cli::debug_sandbox::run_command_under_seatbelt(
                    seatbelt_command,
                    codex_linux_sandbox_exe,
                )
                .await?;
            }
            DebugCommand::Landlock(mut landlock_command) => {
                landlock_command
                    .config_overrides
                    .raw_overrides
                    .splice(0..0, cli.config_overrides.raw_overrides.into_iter());
                codex_cli::debug_sandbox::run_command_under_landlock(
                    landlock_command,
                    codex_linux_sandbox_exe,
                )
                .await?;
            }
        },
    }

    Ok(())
}
