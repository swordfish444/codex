use clap::Parser;
use codex_cli::LandlockCommand;
use codex_cli::SeatbeltCommand;
use codex_cli::login::run_login_with_chatgpt;
use codex_cli::proto;
use codex_common::CliConfigOverrides;
use codex_exec::Cli as ExecCli;
use codex_tui::Cli as TuiCli;
use std::path::PathBuf;
use std::{env, fs, process};
use std::io::ErrorKind;
use toml::{self, value::Table, Value};
use serde::de::Error as SerdeError;
use codex_core::config::find_codex_home;
use uuid::Uuid;

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

/// Parse a raw TOML literal (e.g. `true`, `42`, `[1,2]`, `{a=1}`) into a TOML Value.
/// Wraps the literal under a sentinel key to satisfy the TOML parser.
fn parse_toml_value(raw: &str) -> Result<Value, toml::de::Error> {
    let wrapped = format!("_x_ = {raw}");
    let table: Table = toml::from_str(&wrapped)?;
    table.get("_x_")
        .cloned()
        .ok_or_else(|| SerdeError::custom("missing sentinel"))
}

/// Subcommands for the `codex config` command.
#[derive(Debug, clap::Subcommand)]
enum ConfigCmd {
    /// Open the config file in your editor ($EDITOR or vi).
    Edit,
    /// Set a configuration key to a TOML literal, e.g. `tui.auto_mount_repo true`.
    Set {
        /// Dotted path to the key (e.g. `tui.auto_mount_repo`).
        key: String,
        /// A TOML literal value (e.g. `true`, `42`, `"foo"`, `[1,2]`).
        value: String,
    },
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    /// Resume an existing TUI session by UUID.
    Session {
        /// UUID of the session to resume
        session_id: Uuid,
    },
    /// Inspect or modify the CLI configuration file.
    #[command(subcommand)]
    Config(ConfigCmd),
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
            prepend_config_flags(&mut tui_cli.config_overrides, cli.config_overrides);
            codex_tui::run_main(tui_cli, codex_linux_sandbox_exe)?;
        }
        Some(Subcommand::Session { session_id }) => {
            let mut tui_cli = cli.interactive;
            tui_cli.session = Some(session_id);
            prepend_config_flags(&mut tui_cli.config_overrides, cli.config_overrides);
            codex_tui::run_main(tui_cli, codex_linux_sandbox_exe)?;
        }
        Some(Subcommand::Config(cmd)) => {
            // Handle `codex config` subcommands: edit or set.
            // Determine config directory and file path.
            let codex_home = find_codex_home()?;
            fs::create_dir_all(&codex_home)?;
            let config_path = codex_home.join("config.toml");
            // Load existing config.toml into a Toml value, or start with empty table.
            let mut doc = match fs::read_to_string(&config_path) {
                Ok(s) => toml::from_str::<toml::Value>(&s)?,
                Err(e) if e.kind() == ErrorKind::NotFound => toml::Value::Table(Default::default()),
                Err(e) => return Err(e.into()),
            };
            match cmd {
                ConfigCmd::Edit => {
                    // Ensure the config file exists.
                    if !config_path.exists() {
                        fs::write(&config_path, "")?;
                    }
                    // Open in editor from $EDITOR or fall back to vi.
                    let editor = env::var_os("EDITOR").unwrap_or_else(|| "vi".into());
                    let status = process::Command::new(editor)
                        .arg(&config_path)
                        .status()?;
                    if !status.success() {
                        std::process::exit(status.code().unwrap_or(1));
                    }
                }
                ConfigCmd::Set { key, value } => {
                    // Parse the provided TOML literal value.
                    let val = parse_toml_value(&value)
                        .map_err(|e| anyhow::anyhow!("TOML parse error for `{}`: {}", value, e))?;
                    // Apply the override into the document.
                    apply_override(&mut doc, &key, val);
                    // Serialize and write back to disk.
                    let s = toml::to_string_pretty(&doc)?;
                    fs::write(&config_path, s)?;
                }
            }
            return Ok(());
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

/// Apply a dotted-path override into a TOML document, creating tables as needed.
fn apply_override(root: &mut toml::Value, path: &str, value: toml::Value) {
    use toml::value::Table;
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = root;
    for (i, part) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        if last {
            match current {
                toml::Value::Table(tbl) => {
                    tbl.insert((*part).to_string(), value);
                }
                _ => {
                    let mut tbl = Table::new();
                    tbl.insert((*part).to_string(), value);
                    *current = toml::Value::Table(tbl);
                }
            }
            return;
        }
        match current {
            toml::Value::Table(tbl) => {
                current = tbl.entry((*part).to_string())
                    .or_insert_with(|| toml::Value::Table(Table::new()));
            }
            _ => {
                *current = toml::Value::Table(Table::new());
                if let toml::Value::Table(tbl) = current {
                    current = tbl.entry((*part).to_string())
                        .or_insert_with(|| toml::Value::Table(Table::new()));
                }
            }
        }
    }
}

// ---------------------
// Tests for CLI parsing
// ---------------------
#[cfg(test)]
mod tests {
    use super::MultitoolCli;
    use clap::CommandFactory;

    #[test]
    fn config_subcommands_help() {
        let mut cmd = MultitoolCli::command();
        let cfg = cmd.find_subcommand_mut("config").expect("config subcommand not found");
        let mut buf = Vec::new();
        cfg.write_long_help(&mut buf).unwrap();
        let help = String::from_utf8(buf).unwrap();
        assert!(help.contains("edit"), "help missing 'edit': {}", help);
        assert!(help.contains("set"), "help missing 'set': {}", help);
    }
}
