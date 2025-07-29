// Forbid accidental stdout/stderr writes in the *library* portion of the TUI.
// The standalone `codex-tui` binary prints a short help message before the
// alternate‑screen mode starts; that file opts‑out locally via `allow`.
#![deny(clippy::print_stdout, clippy::print_stderr)]
use app::App;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config_types::SandboxMode;
use codex_core::openai_api_key::OPENAI_API_KEY_ENV_VAR;
use codex_core::openai_api_key::get_openai_api_key;
use codex_core::openai_api_key::set_openai_api_key;
use codex_core::protocol::AskForApproval;
use codex_core::util::is_inside_git_repo;
use codex_core::util::maybe_read_file;
use codex_login::try_read_openai_api_key;
use log_layer::TuiLogLayer;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use tracing_appender::non_blocking;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

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
mod file_search;
mod get_git_diff;
mod git_warning_screen;
mod history_cell;
mod insert_history;
mod log_layer;
mod login_screen;
mod markdown;
mod scroll_event_helper;
mod slash_command;
mod status_indicator_widget;
mod text_block;
mod text_formatting;
mod tui;
mod user_approval_widget;

pub use cli::Cli;

pub fn run_main(
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

    // Capture any read error for experimental instructions so we can log it
    // after the tracing subscriber has been initialized.
    let mut experimental_read_error: Option<String> = None;

    let (config, experimental_prompt_label) = {
        // Load configuration and support CLI overrides.
        // If the experimental instructions flag points at a file, read its
        // contents; otherwise use the value verbatim. Avoid printing to stdout
        // or stderr in this library crate – fallback to the raw string on
        // errors.
        let base_instructions =
            cli.experimental_instructions
                .as_deref()
                .and_then(|s| match maybe_read_file(s) {
                    Ok(v) => v,
                    Err(e) => {
                        experimental_read_error = Some(format!(
                            "Failed to read experimental instructions from '{s}': {e}"
                        ));
                        Some(s.to_string())
                    }
                });

        // Derive a label shown in the welcome banner describing the origin of
        // the experimental instructions: filename for file paths and
        // "experimental" for literals.
        let experimental_prompt_label = cli.experimental_instructions.as_deref().map(|s| {
            let p = Path::new(s);
            if p.is_file() {
                p.file_name()
                    .map(|os| os.to_string_lossy().to_string())
                    .unwrap_or_else(|| s.to_string())
            } else {
                "experimental".to_string()
            }
        });

        // Do not show a label if the file was empty (base_instructions is None).
        let experimental_prompt_label = if base_instructions.is_some() {
            experimental_prompt_label
        } else {
            None
        };

        let overrides = ConfigOverrides {
            model: cli.model.clone(),
            approval_policy,
            sandbox_mode,
            cwd: cli.cwd.clone().map(|p| p.canonicalize().unwrap_or(p)),
            model_provider: None,
            config_profile: cli.config_profile.clone(),
            codex_linux_sandbox_exe,
            base_instructions,
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
            Ok(config) => (config, experimental_prompt_label),
            Err(err) => {
                eprintln!("Error loading configuration: {err}");
                std::process::exit(1);
            }
        }
    };

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

    if let Some(msg) = experimental_read_error {
        // Now that logging is initialized, record a warning so the user
        // can see that Codex fell back to using the literal string.
        tracing::warn!("{msg}");
    }

    let show_login_screen = should_show_login_screen(&config);

    // Determine whether we need to display the "not a git repo" warning
    // modal. The flag is shown when the current working directory is *not*
    // inside a Git repository **and** the user did *not* pass the
    // `--allow-no-git-exec` flag.
    let show_git_warning = !cli.skip_git_repo_check && !is_inside_git_repo(&config);

    run_ratatui_app(
        cli,
        config,
        show_login_screen,
        show_git_warning,
        experimental_prompt_label,
        log_rx,
    )
    .map_err(|err| std::io::Error::other(err.to_string()))
}

fn run_ratatui_app(
    cli: Cli,
    config: Config,
    show_login_screen: bool,
    show_git_warning: bool,
    experimental_prompt_label: Option<String>,
    mut log_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> color_eyre::Result<codex_core::protocol::TokenUsage> {
    color_eyre::install()?;

    // Forward panic reports through tracing so they appear in the UI status
    // line instead of interleaving raw panic output with the interface.
    std::panic::set_hook(Box::new(|info| {
        tracing::error!("panic: {info}");
    }));
    let mut terminal = tui::init(&config)?;
    terminal.clear()?;

    let Cli { prompt, images, .. } = cli;
    let mut app = App::new(
        config.clone(),
        prompt,
        show_login_screen,
        show_git_warning,
        images,
        experimental_prompt_label,
    );

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

#[cfg(test)]
mod tests {
    use codex_core::util::maybe_read_file;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_path() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("codex_tui_test_{}.txt", Uuid::new_v4()));
        p
    }

    #[test]
    fn maybe_read_file_returns_literal_for_non_path() {
        let res = match maybe_read_file("Base instructions as a string") {
            Ok(v) => v,
            Err(e) => panic!("error: {e}"),
        };
        assert_eq!(res, Some("Base instructions as a string".to_string()));
    }

    #[test]
    fn maybe_read_file_reads_and_trims_file_contents() {
        let p = temp_path();
        if let Err(e) = fs::write(&p, "  file text  \n") {
            panic!("write temp file: {e}");
        }
        let p_s = p.to_string_lossy().to_string();
        let res = match maybe_read_file(&p_s) {
            Ok(v) => v,
            Err(e) => panic!("error: {e}"),
        };
        assert_eq!(res, Some("file text".to_string()));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn maybe_read_file_empty_file_returns_none() {
        let p = temp_path();
        if let Err(e) = fs::write(&p, "  \n\t") {
            panic!("write temp file: {e}");
        }
        let p_s = p.to_string_lossy().to_string();
        let res = match maybe_read_file(&p_s) {
            Ok(v) => v,
            Err(e) => panic!("error: {e}"),
        };
        assert_eq!(res, None);
        let _ = std::fs::remove_file(&p);
    }
}
