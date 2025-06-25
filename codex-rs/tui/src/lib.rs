// Forbid accidental stdout/stderr writes in the *library* portion of the TUI.
// The standalone `codex-tui` binary prints a short help message before the
// alternate‑screen mode starts; that file opts‑out locally via `allow`.
#![deny(clippy::print_stdout, clippy::print_stderr)]
use app::App;
use codex_core::ResponseItem;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::openai_api_key::OPENAI_API_KEY_ENV_VAR;
use codex_core::openai_api_key::get_openai_api_key;
use codex_core::openai_api_key::set_openai_api_key;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;
use codex_core::util::is_inside_git_repo;
use codex_login::try_read_openai_api_key;
use log_layer::TuiLogLayer;
use serde_json;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tracing_appender::non_blocking;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use uuid::Uuid;

mod app;
mod app_event;
mod app_event_sender;
mod bottom_pane;
mod cell_widget;
mod chatwidget;
mod citation_regex;
mod cli;
mod conversation_history_widget;
mod exec_command;
mod git_warning_screen;
mod history_cell;
mod log_layer;
mod login_screen;
mod markdown;
mod mouse_capture;
mod scroll_event_helper;
mod slash_command;
mod confirm_ctrl_d;
mod status_indicator_widget;
mod context;
mod text_block;
mod text_formatting;
mod tui;
mod user_approval_widget;
mod config_reload;

pub use cli::Cli;

pub fn run_main(cli: Cli, codex_linux_sandbox_exe: Option<PathBuf>) -> std::io::Result<()> {
    let (sandbox_policy, approval_policy) = if cli.full_auto {
        (
            Some(SandboxPolicy::new_full_auto_policy()),
            Some(AskForApproval::OnFailure),
        )
    } else {
        let sandbox_policy = cli.sandbox.permissions.clone().map(Into::into);
        (sandbox_policy, cli.approval_policy.map(Into::into))
    };

    let config = {
        // Load configuration and support CLI overrides.
        let overrides = ConfigOverrides {
            model: cli.model.clone(),
            approval_policy,
            sandbox_policy,
            cwd: cli.cwd.clone().map(|p| p.canonicalize().unwrap_or(p)),
            model_provider: None,
            config_profile: cli.config_profile.clone(),
            codex_linux_sandbox_exe,
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

    // Determine log file path: use --debug-log if set, otherwise default under config log_dir
    let log_path = if let Some(path) = &cli.debug_log {
        path.clone()
    } else {
        let log_dir = codex_core::config::log_dir(&config)?;
        std::fs::create_dir_all(&log_dir)?;
        log_dir.join("codex-tui.log")
    };
    // Open (or create) your log file, appending to it.
    let mut log_file_opts = OpenOptions::new();
    log_file_opts.create(true).append(true);

    // Ensure the file is only readable and writable by the current user on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        log_file_opts.mode(0o600);
    }

    let log_file = log_file_opts.open(log_path)?;

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

    let show_login_screen = should_show_login_screen(&config);

    // Determine whether we need to display the "not a git repo" warning
    // modal. The flag is shown when the current working directory is *not*
    // inside a Git repository **and** the user did *not* pass the
    // `--allow-no-git-exec` flag.
    let show_git_warning = !cli.skip_git_repo_check && !is_inside_git_repo(&config);

    try_run_ratatui_app(cli, config, show_login_screen, show_git_warning, log_rx);
    Ok(())
}

#[expect(
    clippy::print_stderr,
    reason = "Resort to stderr in exceptional situations."
)]
fn try_run_ratatui_app(
    cli: Cli,
    config: Config,
    show_login_screen: bool,
    show_git_warning: bool,
    log_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    if let Err(report) = run_ratatui_app(cli, config, show_login_screen, show_git_warning, log_rx) {
        eprintln!("Error: {report:?}");
    }
}

fn run_ratatui_app(
    cli: Cli,
    config: Config,
    show_login_screen: bool,
    show_git_warning: bool,
    mut log_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> color_eyre::Result<()> {
    color_eyre::install()?;

    // Forward panic reports through the tracing stack so that they appear in
    // the status indicator instead of breaking the alternate screen – the
    // normal colour‑eyre hook writes to stderr which would corrupt the UI.
    std::panic::set_hook(Box::new(|info| {
        tracing::error!("panic: {info}");
    }));
    let (mut terminal, mut mouse_capture) = tui::init(&config)?;
    terminal.clear()?;

    let Cli {
        prompt,
        images,
        session,
        ..
    } = cli;
    let mut app = App::new(
        config.clone(),
        prompt,
        show_login_screen,
        show_git_warning,
        images,
    );
    // If resuming, override the generated session ID so UI hint logic and history use it
    if let Some(id) = session {
        app.set_session_id(id);
        // Attempt to replay past conversation transcript
        if let Some(items) = load_rollout_for_session(&config, id) {
            app.replay_items(items);
        }
    }

    // Bridge log receiver into the AppEvent channel so latest log lines update the UI.
    {
        let app_event_tx = app.event_sender();
        tokio::spawn(async move {
            while let Some(line) = log_rx.recv().await {
                app_event_tx.send(crate::app_event::AppEvent::LatestLog(line));
            }
        });
    }

    // Watch config.toml for changes and prompt reload.
    {
        let app_event_tx = app.event_sender();
        let config_path = config.codex_home.join("config.toml");
        std::thread::spawn(move || {
            use notify::{Watcher, RecursiveMode, RecommendedWatcher, EventKind};
            use std::sync::mpsc::channel;
            use std::time::Duration;
            let (tx, rx) = channel();
            let mut watcher: RecommendedWatcher =
                Watcher::new(tx, notify::Config::default()).unwrap_or_else(|e| {
                    tracing::error!("config watcher failed: {e}");
                    std::process::exit(1);
                });
            if watcher.watch(&config_path, RecursiveMode::NonRecursive).is_err() {
                tracing::error!("Failed to watch config.toml");
                return;
            }
            let mut last = std::fs::read_to_string(&config_path).unwrap_or_default();
            for res in rx {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_)) {
                        std::thread::sleep(Duration::from_millis(100));
                        let new = std::fs::read_to_string(&config_path).unwrap_or_default();
                        if new != last {
                            let diff = crate::config_reload::generate_diff(&last, &new);
                            last = new.clone();
                            app_event_tx.send(
                                crate::app_event::AppEvent::ConfigReloadRequest(diff)
                            );
                        }
                    }
                }
            }
        });
    }

    let app_result = app.run(&mut terminal, &mut mouse_capture);

    restore();
    // On exit, display a command that can be used to resume this session
    #[allow(clippy::print_stderr)]
    if let Some(session_id) = app.session_id() {
        eprintln!("Resume this session with: codex session {session_id}");
    }
    app_result
}

/// Load and parse a previous session's rollout JSONL file.
fn load_rollout_for_session(config: &Config, session_id: Uuid) -> Option<Vec<ResponseItem>> {
    let dir = config.codex_home.join("sessions");
    let target = session_id.to_string();
    for entry in fs::read_dir(&dir).ok()? {
        let path = entry.ok()?.path();
        if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
            if fname.starts_with("rollout-") && fname.contains(&target) && fname.ends_with(".jsonl")
            {
                let file = File::open(path).ok()?;
                let reader = BufReader::new(file);
                let mut items = Vec::new();
                for line in reader.lines().flatten() {
                    if let Ok(item) = serde_json::from_str::<ResponseItem>(&line) {
                        items.push(item);
                    }
                }
                return Some(items);
            }
        }
    }
    None
}

#[expect(
    clippy::print_stderr,
    reason = "TUI should no longer be displayed, so we can write to stderr."
)]
fn restore() {
    if let Err(err) = tui::restore() {
        eprintln!(
            "failed to restore terminal. Run `reset` or restart your terminal to recover: {}",
            err
        );
    }
}

#[allow(clippy::unwrap_used)]
fn should_show_login_screen(config: &Config) -> bool {
    if is_in_need_of_openai_api_key(config) {
        // Reading the OpenAI API key is an async operation because it may need
        // to refresh the token. Block on it.
        let codex_home = config.codex_home.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            match try_read_openai_api_key(&codex_home).await {
                Ok(openai_api_key) => {
                    set_openai_api_key(openai_api_key);
                    tx.send(false).unwrap();
                }
                Err(_) => {
                    tx.send(true).unwrap();
                }
            }
        });
        // TODO(mbolin): Impose some sort of timeout.
        tokio::task::block_in_place(|| rx.blocking_recv()).unwrap()
    } else {
        false
    }
}

fn is_in_need_of_openai_api_key(config: &Config) -> bool {
    let is_using_openai_key = config
        .model_provider
        .env_key
        .as_ref()
        .map(|s| s == OPENAI_API_KEY_ENV_VAR)
        .unwrap_or(false);
    is_using_openai_key && get_openai_api_key().is_none()
}
