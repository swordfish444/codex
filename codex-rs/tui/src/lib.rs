// Forbid accidental stdout/stderr writes in the *library* portion of the TUI.
// The standalone `codex-tui` binary prints a short help message before the
// alternate‑screen mode starts; that file opts‑out locally via `allow`.
#![deny(clippy::print_stdout, clippy::print_stderr)]
use app::App;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config_types::SandboxMode;
use codex_core::protocol::AskForApproval;
use codex_core::util::is_inside_git_repo;
use codex_login::load_auth;
use crossterm::event::Event as CEvent;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use crossterm::event::{self};
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use log_layer::TuiLogLayer;
use std::fs::OpenOptions;
use std::io::Write;
use std::io::{self};
use std::path::PathBuf;
use tracing::error;
use tracing_appender::non_blocking;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

mod app;
mod app_event;
mod app_event_sender;
mod bottom_pane;
mod chatwidget;
mod citation_regex;
mod cli;
mod custom_terminal;
mod exec_command;
mod file_search;
mod get_git_diff;
mod git_warning_screen;
mod history_cell;
mod insert_history;
mod log_layer;
mod markdown;
mod slash_command;
mod status_indicator_widget;
mod text_block;
mod text_formatting;
mod tui;
mod user_approval_widget;

#[cfg(not(debug_assertions))]
mod updates;
#[cfg(not(debug_assertions))]
use color_eyre::owo_colors::OwoColorize;

pub use cli::Cli;
// Centralized Ollama helpers from core
use codex_core::providers::ollama::OllamaClient;
use codex_core::providers::ollama::TuiProgressReporter;
use codex_core::providers::ollama::{ensure_configured_and_running, ensure_model_available};
use codex_core::providers::ollama::{read_config_models, read_provider_state, write_config_models};

fn print_inline_message_no_models(
    host_root: &str,
    config_path: &std::path::Path,
    provider_was_present_before: bool,
) -> io::Result<()> {
    let mut out = std::io::stdout();
    let path = config_path.display().to_string();
    // green bold helper
    let b = |s: &str| format!("\x1b[1m{s}\x1b[0m");
    // Ensure we start clean at column 0.
    out.write_all(b"\r\x1b[2K")?;
    out.write_all(
        format!(
            "{}\n\n",
            b("we've discovered no models on your local Ollama instance.")
        )
        .as_bytes(),
    )?;
    out.write_all(format!("\rendpoint: {host_root}\n").as_bytes())?;
    if provider_was_present_before {
        out.write_all(format!("\rconfig: ollama provider already present in {path}\n").as_bytes())?;
    } else {
        out.write_all(
            format!("\rconfig: added ollama as a model provider in {path}\n").as_bytes(),
        )?;
    }
    out.write_all(
        b"\rmodels: none recorded in config (pull models with `ollama pull <model>`).\n\n",
    )?;
    out.flush()
}

fn run_inline_models_picker(
    host_root: &str,
    available: &[String],
    preselected: &[String],
    config_path: &std::path::Path,
    provider_was_present_before: bool,
) -> io::Result<()> {
    let mut out = std::io::stdout();
    let mut selected: Vec<bool> = available
        .iter()
        .map(|m| preselected.iter().any(|x| x == m))
        .collect();
    let mut cursor: usize = 0;

    let mut first = true;
    let mut lines_printed: usize = 0;

    enable_raw_mode()?;

    loop {
        // Render block
        render_inline_picker(
            &mut out,
            host_root,
            available,
            &selected,
            cursor,
            &mut first,
            &mut lines_printed,
        )?;

        // Wait for key
        match event::read()? {
            CEvent::Key(KeyEvent {
                code: KeyCode::Up, ..
            })
            | CEvent::Key(KeyEvent {
                code: KeyCode::Char('k'),
                ..
            }) => {
                cursor = cursor.saturating_sub(1);
            }
            CEvent::Key(KeyEvent {
                code: KeyCode::Down,
                ..
            })
            | CEvent::Key(KeyEvent {
                code: KeyCode::Char('j'),
                ..
            }) => {
                if cursor + 1 < available.len() {
                    cursor += 1;
                }
            }
            CEvent::Key(KeyEvent {
                code: KeyCode::Char(' '),
                ..
            }) => {
                if let Some(s) = selected.get_mut(cursor) {
                    *s = !*s;
                }
            }
            CEvent::Key(KeyEvent {
                code: KeyCode::Char('a'),
                ..
            }) => {
                let all_sel = selected.iter().all(|s| *s);
                selected.fill(!all_sel);
            }
            // Allow quitting the entire app from the inline picker with Ctrl+C or Ctrl+D.
            CEvent::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => {
                // Restore terminal state and exit with SIGINT-like code.
                disable_raw_mode()?;
                // Start on a clean line before exiting.
                out.write_all(b"\r\x1b[2K\n")?;
                std::process::exit(130);
            }
            CEvent::Key(KeyEvent {
                code: KeyCode::Char('d'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => {
                // Restore terminal state and exit cleanly.
                disable_raw_mode()?;
                out.write_all(b"\r\x1b[2K\n")?;
                std::process::exit(0);
            }
            CEvent::Key(KeyEvent {
                code: KeyCode::Enter,
                ..
            }) => {
                break;
            }
            CEvent::Key(KeyEvent {
                code: KeyCode::Char('q'),
                ..
            })
            | CEvent::Key(KeyEvent {
                code: KeyCode::Esc, ..
            }) => {
                // Skip saving – print summary and continue.
                disable_raw_mode()?;
                print_config_summary_after_save(config_path, provider_was_present_before, None)?;
                return Ok(());
            }
            _ => {}
        }
    }

    disable_raw_mode()?;
    // Ensure the summary starts on a clean, left‑aligned new line.
    out.write_all(b"\r\x1b[2K\n")?;

    // Compute chosen
    let chosen: Vec<String> = available
        .iter()
        .cloned()
        .zip(selected.iter())
        .filter_map(|(name, sel)| if *sel { Some(name) } else { None })
        .collect();

    let _ = write_config_models(config_path, &chosen);
    print_config_summary_after_save(config_path, provider_was_present_before, Some(chosen.len()))
}

fn render_inline_picker(
    out: &mut std::io::Stdout,
    host_root: &str,
    items: &[String],
    selected: &[bool],
    cursor: usize,
    first: &mut bool,
    lines_printed: &mut usize,
) -> io::Result<()> {
    // If not first render, move to the start of the block. We will clear each line as we redraw.
    if !*first {
        out.write_all(format!("\x1b[{}A", *lines_printed).as_bytes())?; // up N lines
        // Ensure we start at column 1 for a clean redraw.
        out.write_all(b"\r")?;
    }

    let mut lines = Vec::new();
    let bold = |s: &str| format!("\x1b[1m{s}\x1b[0m");
    lines.push(bold(&format!("discovered models on ollama ({host_root}):")));
    lines
        .push("↑/↓ move, space to toggle, 'a' (un)select all, enter confirm, 'q' skip".to_string());
    lines.push(String::new());
    for (i, name) in items.iter().enumerate() {
        let mark = if selected.get(i).copied().unwrap_or(false) {
            "\x1b[32m[x]\x1b[0m" // green
        } else {
            "[ ]"
        };
        let mut line = format!("{mark} {name}");
        if i == cursor {
            line = format!("\x1b[7m{line}\x1b[0m"); // reverse video for current row
        }
        lines.push(line);
    }

    for l in &lines {
        // Move to column 0 and clear the entire line before writing.
        out.write_all(b"\r\x1b[2K")?;
        out.write_all(l.as_bytes())?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    *first = false;
    *lines_printed = lines.len();
    Ok(())
}

fn print_config_summary_after_save(
    config_path: &std::path::Path,
    provider_was_present_before: bool,
    models_count_after: Option<usize>,
) -> io::Result<()> {
    let mut out = std::io::stdout();
    // Start clean and at column 0
    out.write_all(b"\r\x1b[2K")?;
    let path = config_path.display().to_string();
    if provider_was_present_before {
        out.write_all(format!("\rconfig: ollama provider already present in {path}\n").as_bytes())?;
    } else {
        out.write_all(
            format!("\rconfig: added ollama as a model provider in {path}\n").as_bytes(),
        )?;
    }
    if let Some(after) = models_count_after {
        let names = read_config_models(config_path).unwrap_or_default();
        if names.is_empty() {
            out.write_all(format!("\rmodels: recorded {after}\n\n").as_bytes())?;
        } else {
            out.write_all(
                format!("\rmodels: recorded {} ({})\n\n", after, names.join(", ")).as_bytes(),
            )?;
        }
    } else {
        out.write_all(b"\rmodels: no changes recorded\n\n")?;
    }
    out.flush()
}

pub async fn run_main(
    cli: Cli,
    codex_linux_sandbox_exe: Option<PathBuf>,
) -> std::io::Result<codex_core::protocol::TokenUsage> {
    let (sandbox_mode, approval_policy) = if cli.full_auto {
        (
            Some(SandboxMode::WorkspaceWrite),
            Some(AskForApproval::OnFailure),
        )
    } else if cli.dangerously_bypass_approvals_and_sandbox {
        (
            Some(SandboxMode::DangerFullAccess),
            Some(AskForApproval::Never),
        )
    } else {
        (
            cli.sandbox_mode.map(Into::<SandboxMode>::into),
            cli.approval_policy.map(Into::into),
        )
    };

    // Track config.toml state for messaging before launching TUI.
    let provider_was_present_before = if cli.ollama {
        let codex_home = codex_core::config::find_codex_home()?;
        let config_path = codex_home.join("config.toml");
        let (p, _m) = read_provider_state(&config_path);
        p
    } else {
        false
    };

    let config = {
        // If the user selected the Ollama provider via `--ollama`, verify a
        // local server is reachable and ensure a provider entry exists in
        // config.toml. Exit early with a helpful message otherwise.
        if cli.ollama {
            if let Err(e) = ensure_configured_and_running().await {
                #[allow(clippy::print_stderr)]
                {
                    eprintln!("{e}");
                }
                std::process::exit(1);
            }
        }

        // Load configuration and support CLI overrides.
        let overrides = ConfigOverrides {
            model: cli.model.clone(),
            approval_policy,
            sandbox_mode,
            cwd: cli.cwd.clone().map(|p| p.canonicalize().unwrap_or(p)),
            model_provider: if cli.ollama {
                Some("ollama".to_string())
            } else {
                None
            },
            config_profile: cli.config_profile.clone(),
            codex_linux_sandbox_exe,
            base_instructions: None,
            include_plan_tool: Some(true),
        };
        // Parse `-c` overrides from the CLI.
        let cli_kv_overrides = match cli.config_overrides.parse_overrides() {
            Ok(v) => v,
            #[allow(clippy::print_stderr)]
            Err(e) => {
                eprintln!("Error parsing -c overrides: {e}");
                std::process::exit(1);
            }
        };

        #[allow(clippy::print_stderr)]
        match Config::load_with_cli_overrides(cli_kv_overrides, overrides) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("Error loading configuration: {err}");
                std::process::exit(1);
            }
        }
    };
    // If the user passed --ollama, either ensure an explicitly requested model is
    // available (automatic pull if allowlisted) or offer an inline picker when no
    // specific model was provided.
    if cli.ollama {
        // Determine host root for the Ollama native API (e.g. http://localhost:11434).
        let base_url = config
            .model_provider
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
        let host_root = base_url
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .to_string();
        let config_path = config.codex_home.join("config.toml");

        if let Some(ref model_name) = cli.model {
            // Explicit model requested: ensure it is available locally without prompting.
            let client = OllamaClient::from_provider(&config.model_provider);
            let mut reporter = TuiProgressReporter::new();
            if let Err(e) =
                ensure_model_available(model_name, &client, &config_path, &mut reporter).await
            {
                let mut out = std::io::stderr();
                let _ = out.write_all(format!("{e}\n").as_bytes());
                let _ = out.flush();
                std::process::exit(1);
            }
        } else {
            // No specific model was requested: fetch available models from the local instance
            // and, if they differ from what is listed in config.toml, display a minimal
            // inline selection UI before launching the TUI.
            let client = OllamaClient::from_provider(&config.model_provider);
            let available_models: Vec<String> = client.fetch_models().await.unwrap_or_default();

            // Read existing models in config.
            let existing_models: Vec<String> = read_config_models(&config_path).unwrap_or_default();

            if available_models.is_empty() {
                // Inform the user and continue launching the TUI.
                print_inline_message_no_models(
                    &host_root,
                    &config_path,
                    provider_was_present_before,
                )?;
            } else {
                // Compare sets to decide whether to show the prompt.
                let set_eq = {
                    use std::collections::HashSet;
                    let a: HashSet<_> = available_models.iter().collect();
                    let b: HashSet<_> = existing_models.iter().collect();
                    a == b
                };

                if !set_eq {
                    run_inline_models_picker(
                        &host_root,
                        &available_models,
                        &existing_models,
                        &config_path,
                        provider_was_present_before,
                    )?;
                }
            }
        }
    }

    let log_dir = codex_core::config::log_dir(&config)?;
    std::fs::create_dir_all(&log_dir)?;
    // Open (or create) your log file, appending to it.
    let mut log_file_opts = OpenOptions::new();
    log_file_opts.create(true).append(true);

    // Ensure the file is only readable and writable by the current user.
    // Doing the equivalent to `chmod 600` on Windows is quite a bit more code
    // and requires the Windows API crates, so we can reconsider that when
    // Codex CLI is officially supported on Windows.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        log_file_opts.mode(0o600);
    }

    let log_file = log_file_opts.open(log_dir.join("codex-tui.log"))?;

    // Wrap file in non‑blocking writer.
    let (non_blocking, _guard) = non_blocking(log_file);

    // use RUST_LOG env var, default to info for codex crates.
    let env_filter = || {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("codex_core=info,codex_tui=info"))
    };

    // Build layered subscriber:
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_target(false)
        .with_filter(env_filter());

    // Channel that carries formatted log lines to the UI.
    let (log_tx, log_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let tui_layer = TuiLogLayer::new(log_tx.clone(), 120).with_filter(env_filter());

    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(tui_layer)
        .try_init();

    #[allow(clippy::print_stderr)]
    #[cfg(not(debug_assertions))]
    if let Some(latest_version) = updates::get_upgrade_version(&config) {
        let current_version = env!("CARGO_PKG_VERSION");
        let exe = std::env::current_exe()?;
        let managed_by_npm = std::env::var_os("CODEX_MANAGED_BY_NPM").is_some();

        eprintln!(
            "{} {current_version} -> {latest_version}.",
            "✨⬆️ Update available!".bold().cyan()
        );

        if managed_by_npm {
            let npm_cmd = "npm install -g @openai/codex@latest";
            eprintln!("Run {} to update.", npm_cmd.cyan().on_black());
        } else if cfg!(target_os = "macos")
            && (exe.starts_with("/opt/homebrew") || exe.starts_with("/usr/local"))
        {
            let brew_cmd = "brew upgrade codex";
            eprintln!("Run {} to update.", brew_cmd.cyan().on_black());
        } else {
            eprintln!(
                "See {} for the latest releases and installation options.",
                "https://github.com/openai/codex/releases/latest"
                    .cyan()
                    .on_black()
            );
        }

        eprintln!("");
    }

    let show_login_screen = should_show_login_screen(&config);
    if show_login_screen {
        std::io::stdout()
            .write_all(b"No API key detected.\nLogin with your ChatGPT account? [Yn] ")?;
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if !(trimmed.is_empty() || trimmed.eq_ignore_ascii_case("y")) {
            std::process::exit(1);
        }
        // Spawn a task to run the login command.
        // Block until the login command is finished.
        codex_login::login_with_chatgpt(&config.codex_home, false).await?;

        std::io::stdout().write_all(b"Login successful.\n")?;
    }

    // Determine whether we need to display the "not a git repo" warning
    // modal. The flag is shown when the current working directory is *not*
    // inside a Git repository **and** the user did *not* pass the
    // `--allow-no-git-exec` flag.
    let show_git_warning = !cli.skip_git_repo_check && !is_inside_git_repo(&config);

    run_ratatui_app(cli, config, show_git_warning, log_rx)
        .map_err(|err| std::io::Error::other(err.to_string()))
}

fn run_ratatui_app(
    cli: Cli,
    config: Config,
    show_git_warning: bool,
    mut log_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> color_eyre::Result<codex_core::protocol::TokenUsage> {
    color_eyre::install()?;

    // Forward panic reports through tracing so they appear in the UI status
    // line, but do not swallow the default/color-eyre panic handler.
    // Chain to the previous hook so users still get a rich panic report
    // (including backtraces) after we restore the terminal.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        prev_hook(info);
    }));
    let mut terminal = tui::init(&config)?;
    terminal.clear()?;

    let Cli { prompt, images, .. } = cli;
    let mut app = App::new(config.clone(), prompt, show_git_warning, images);

    // Bridge log receiver into the AppEvent channel so latest log lines update the UI.
    {
        let app_event_tx = app.event_sender();
        tokio::spawn(async move {
            while let Some(line) = log_rx.recv().await {
                app_event_tx.send(crate::app_event::AppEvent::LatestLog(line));
            }
        });
    }

    let app_result = app.run(&mut terminal);
    let usage = app.token_usage();

    restore();
    // ignore error when collecting usage – report underlying error instead
    app_result.map(|_| usage)
}

#[expect(
    clippy::print_stderr,
    reason = "TUI should no longer be displayed, so we can write to stderr."
)]
fn restore() {
    if let Err(err) = tui::restore() {
        eprintln!(
            "failed to restore terminal. Run `reset` or restart your terminal to recover: {err}"
        );
    }
}

#[allow(clippy::unwrap_used)]
fn should_show_login_screen(config: &Config) -> bool {
    if config.model_provider.requires_auth {
        // Reading the OpenAI API key is an async operation because it may need
        // to refresh the token. Block on it.
        let codex_home = config.codex_home.clone();
        match load_auth(&codex_home, true) {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(err) => {
                error!("Failed to read auth.json: {err}");
                true
            }
        }
    } else {
        false
    }
}
