use clap::CommandFactory;
use clap::Parser;
use clap_complete::Shell;
use clap_complete::generate;
use codex_chatgpt::apply_command::ApplyCommand;
use codex_chatgpt::apply_command::run_apply_command;
use codex_cli::concurrent::maybe_spawn_concurrent;
use codex_cli::LandlockCommand;
use codex_cli::SeatbeltCommand;
use codex_cli::login::run_login_with_chatgpt;
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

    /// Autonomous mode: run the command in the background & concurrently using a git worktree.
    /// Requires the current directory (or --cd provided path) to be a git repository.
    #[clap(long)]
    concurrent: bool,

    /// Control whether the concurrent run auto-merges the worktree branch back into the original branch.
    /// Defaults to true (may also be set via CONCURRENT_AUTOMERGE env var).
    #[clap(long = "concurrent-automerge", value_name = "BOOL")]
    concurrent_automerge: Option<bool>,

    /// Explicit branch name to use for the concurrent worktree instead of the default `codex/<slug>`.
    /// May also be set via CONCURRENT_BRANCH_NAME env var.
    #[clap(long = "concurrent-branch-name", value_name = "BRANCH")]
    concurrent_branch_name: Option<String>,

    /// Best-of-n: run n concurrent worktrees (1-4) and let user pick the best result. Implies --concurrent and disables automerge.
    #[clap(long = "best-of-n", short = 'n', value_name = "N", default_value_t = 1)]
    pub best_of_n: u8,

    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    /// Run Codex non-interactively.
    #[clap(visible_alias = "e")]
    Exec(ExecCli),

    /// Login with ChatGPT.
    Login(LoginCommand),

    /// Experimental: run Codex as an MCP server.
    Mcp,

    /// Run the Protocol stream via stdin/stdout
    #[clap(visible_alias = "p")]
    Proto(ProtoCli),

    /// Generate shell completion scripts.
    Completion(CompletionCommand),

    /// Internal debugging commands.
    Debug(DebugArgs),

    /// Apply the latest diff produced by Codex agent as a `git apply` to your local working tree.
    #[clap(visible_alias = "a")]
    Apply(ApplyCommand),

    /// Manage / inspect concurrent background tasks.
    Tasks(codex_cli::tasks::TasksCli),

    /// Show or follow logs for a specific task.
    Logs(codex_cli::logs::LogsCli),

    /// Inspect full metadata for a task.
    Inspect(codex_cli::inspect::InspectCli),
}

#[derive(Debug, Parser)]
struct CompletionCommand {
    /// Shell to generate completions for
    #[clap(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
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
struct LoginCommand {
    #[clap(skip)]
    config_overrides: CliConfigOverrides,
}

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
            let mut tui_cli = cli.interactive;
            let root_raw_overrides = cli.config_overrides.raw_overrides.clone();
            prepend_config_flags(&mut tui_cli.config_overrides, cli.config_overrides);
            // Best-of-n logic
            if cli.best_of_n > 1 {
                let n = cli.best_of_n.min(4).max(1);
                let mut spawned_any = false;
                let base_branch = if let Some(ref name) = cli.concurrent_branch_name {
                    name.trim().to_string()
                } else {
                    // Derive slug from prompt (copied from maybe_spawn_concurrent)
                    let raw_prompt = tui_cli.prompt.as_deref().unwrap_or("");
                    let snippet = raw_prompt.chars().take(32).collect::<String>();
                    let mut slug: String = snippet
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
                        .collect();
                    while slug.contains("--") { slug = slug.replace("--", "-"); }
                    slug = slug.trim_matches('-').to_string();
                    if slug.is_empty() { slug = "prompt".into(); }
                    format!("codex/{}", slug)
                };
                for i in 1..=n {
                    let mut tui_cli_n = tui_cli.clone();
                    // Suffix branch name with -01, -02, etc.
                    let branch_name = format!("{}-{:02}", base_branch, i);
                    let branch_name_opt = Some(branch_name);
                    // Always automerge = false for best-of-n
                    match maybe_spawn_concurrent(
                        &mut tui_cli_n,
                        &root_raw_overrides,
                        true, // force concurrent
                        Some(false),
                        &branch_name_opt,
                    ) {
                        Ok(true) => { spawned_any = true; },
                        Ok(false) => {},
                        Err(e) => { eprintln!("Error spawning best-of-n run {}: {e}", i); },
                    }
                }
                if !spawned_any {
                    codex_tui::run_main(tui_cli, codex_linux_sandbox_exe)?;
                }
                // If any spawned, do not run TUI (user will see task IDs)
            } else {
                // Attempt concurrent background spawn; if it returns true we skip launching the TUI.
                if let Ok(spawned) = maybe_spawn_concurrent(
                    &mut tui_cli,
                    &root_raw_overrides,
                    cli.concurrent,
                    cli.concurrent_automerge,
                    &cli.concurrent_branch_name,
                ) {
                    if !spawned { codex_tui::run_main(tui_cli, codex_linux_sandbox_exe)?; }
                } else {
                    // On error fallback to interactive.
                    codex_tui::run_main(tui_cli, codex_linux_sandbox_exe)?;
                }
            }
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            prepend_config_flags(&mut exec_cli.config_overrides, cli.config_overrides);
            codex_exec::run_main(exec_cli, codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::Mcp) => {
            codex_mcp_server::run_main(codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::Login(mut login_cli)) => {
            prepend_config_flags(&mut login_cli.config_overrides, cli.config_overrides);
            run_login_with_chatgpt(login_cli.config_overrides).await;
        }
        Some(Subcommand::Proto(mut proto_cli)) => {
            prepend_config_flags(&mut proto_cli.config_overrides, cli.config_overrides);
            proto::run_main(proto_cli).await?;
        }
        Some(Subcommand::Completion(completion_cli)) => {
            print_completion(completion_cli);
        }
        Some(Subcommand::Debug(debug_args)) => match debug_args.cmd {
            DebugCommand::Seatbelt(mut seatbelt_cli) => {
                prepend_config_flags(&mut seatbelt_cli.config_overrides, cli.config_overrides);
                codex_cli::debug_sandbox::run_command_under_seatbelt(
                    seatbelt_cli,
                    codex_linux_sandbox_exe,
                )
                .await?;
            }
            DebugCommand::Landlock(mut landlock_cli) => {
                prepend_config_flags(&mut landlock_cli.config_overrides, cli.config_overrides);
                codex_cli::debug_sandbox::run_command_under_landlock(
                    landlock_cli,
                    codex_linux_sandbox_exe,
                )
                .await?;
            }
        },
        Some(Subcommand::Apply(mut apply_cli)) => {
            prepend_config_flags(&mut apply_cli.config_overrides, cli.config_overrides);
            run_apply_command(apply_cli).await?;
        }
        Some(Subcommand::Tasks(tasks_cli)) => {
            codex_cli::tasks::run_tasks(tasks_cli)?;
        }
        Some(Subcommand::Logs(logs_cli)) => {
            codex_cli::logs::run_logs(logs_cli)?;
        }
        Some(Subcommand::Inspect(inspect_cli)) => {
            codex_cli::inspect::run_inspect(inspect_cli)?;
        }
    }

    Ok(())
}

/// Prepend root-level overrides so they have lower precedence than
/// CLI-specific ones specified after the subcommand (if any).
fn prepend_config_flags(
    subcommand_config_overrides: &mut CliConfigOverrides,
    cli_config_overrides: CliConfigOverrides,
) {
    subcommand_config_overrides
        .raw_overrides
        .splice(0..0, cli_config_overrides.raw_overrides);
}

fn print_completion(cmd: CompletionCommand) {
    let mut app = MultitoolCli::command();
    let name = "codex";
    generate(cmd.shell, &mut app, name, &mut std::io::stdout());
}
