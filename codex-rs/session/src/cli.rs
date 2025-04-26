//! CLI command definitions and implementation for `codex-session`.
//!
//! The session manager can spawn two different Codex agent flavors:
//!
//! * `codex-exec` – non-interactive batch agent (legacy behaviour)
//! * `codex-repl` – interactive REPL that requires user input after launch
//!
//! The `create` command therefore has mutually exclusive sub-commands so the appropriate
//! arguments can be forwarded to the underlying agent binaries.

use crate::spawn;
use crate::store;
use anyhow::Context;
use anyhow::Result;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use serde::Serialize;

/// A human-friendly representation of a byte count (e.g. 1.4M).
pub fn human_bytes(b: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = b as f64;
    if f >= GB {
        format!("{:.1}G", f / GB)
    } else if f >= MB {
        format!("{:.1}M", f / MB)
    } else if f >= KB {
        format!("{:.1}K", f / KB)
    } else {
        format!("{}B", b)
    }
}

// -----------------------------------------------------------------------------
// Top-level CLI definition

#[derive(Parser)]
#[command(
    name = "codex-session",
    about = "Manage background Codex agent sessions"
)]
pub struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

impl Cli {
    pub async fn dispatch(self) -> Result<()> {
        match self.cmd {
            Commands::Create(x) => x.run().await,
            Commands::Attach(x) => x.run().await,
            Commands::Delete(x) => x.run().await,
            Commands::Logs(x) => x.run().await,
            Commands::List(x) => x.run().await,
            Commands::Mux(x) => x.run().await,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Spawn a new background session.
    Create(CreateCmd),
    /// Attach the current terminal to a running interactive session.
    Attach(AttachCmd),
    /// Terminate a session and remove its on-disk state.
    Delete(DeleteCmd),
    /// Show (and optionally follow) the stdout / stderr logs of a session.
    Logs(LogsCmd),
    /// List all known sessions.
    List(ListCmd),

    /// Internal helper process: PTY multiplexer daemon (hidden).
    #[command(hide = true, name = "__mux")]
    Mux(MuxCmd),
}

// -----------------------------------------------------------------------------
// create

#[derive(Subcommand)]
enum AgentKind {
    /// Non-interactive execution agent.
    Exec(ExecCreateCmd),

    /// Interactive Read-Eval-Print-Loop agent.
    Repl(ReplCreateCmd),

    /// Full-screen Terminal User Interface agent.
    Tui(TuiCreateCmd),
}

#[derive(Args)]
pub struct CreateCmd {
    /// Explicit session name. If omitted, a memorable random one is generated.
    #[arg(long)]
    id: Option<String>,

    #[command(subcommand)]
    agent: AgentKind,
}

#[derive(Args)]
pub struct ExecCreateCmd {
    #[clap(flatten)]
    exec_cli: codex_exec::Cli,
}

#[derive(Args)]
pub struct ReplCreateCmd {
    #[clap(flatten)]
    repl_cli: codex_repl::Cli,
}

#[derive(Args)]
pub struct TuiCreateCmd {
    #[clap(flatten)]
    tui_cli: codex_tui::Cli,
}

impl CreateCmd {
    pub async fn run(self) -> Result<()> {
        let id = match &self.id {
            Some(explicit) => explicit.clone(),
            None => generate_session_id()?,
        };

        let paths = store::paths_for(&id)?;
        store::prepare_dirs(&paths)?;

        // Spawn underlying agent
        let (pid, prompt_preview, kind): (u32, Option<String>, store::SessionKind) = match self.agent {
            AgentKind::Exec(cmd) => {
                let args = build_exec_args(&cmd.exec_cli);
                let child = spawn::spawn_exec(&paths, &args)?;
                let preview = cmd.exec_cli.prompt.as_ref().map(|p| truncate_preview(p));
                (child.id().unwrap_or_default(), preview, store::SessionKind::Exec)
            }
            AgentKind::Repl(cmd) => {
                let args = build_repl_args(&cmd.repl_cli);
                let child = spawn::spawn_repl(&paths, &args)?;
                let preview = cmd.repl_cli.prompt.as_ref().map(|p| truncate_preview(p));
                (child.id().unwrap_or_default(), preview, store::SessionKind::Repl)
            }
            AgentKind::Tui(cmd) => {
                let args = build_tui_args(&cmd.tui_cli);
                let child = spawn::spawn_tui(&paths, &args).await?;
                let preview = cmd.tui_cli.prompt.as_ref().map(|p| truncate_preview(p));
                (child.id().unwrap_or_default(), preview, store::SessionKind::Tui)
            }
        };

        // Persist metadata **after** the process has been spawned so we can record its PID.
        let meta = store::SessionMeta {
            id: id.clone(),
            pid,
            kind,
            created_at: chrono::Utc::now(),
            prompt_preview,
        };
        store::write_meta(&paths, &meta)?;

        println!("{id}");
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// internal mux helper sub-command (hidden)

#[derive(Args)]
pub struct MuxCmd {
    /// Raw PTY master file descriptor passed from the parent process.
    #[arg(long)]
    fd: i32,

    /// Path to the Unix-domain socket that clients attach to.
    #[arg(long)]
    sock: std::path::PathBuf,

    /// Path to the binary stdout log file.
    #[arg(long)]
    log: std::path::PathBuf,
}

impl MuxCmd {
    pub async fn run(self) -> Result<()> {
        crate::spawn::mux_main(self.fd, self.sock, self.log).await
    }
}

fn truncate_preview(p: &str) -> String {
    let slice: String = p.chars().take(40).collect();
    if p.len() > 40 {
        format!("{}…", slice)
    } else {
        slice
    }
}

fn generate_session_id() -> Result<String> {
    let mut generator = names::Generator::with_naming(names::Name::Numbered);
    loop {
        let candidate = generator.next().unwrap();
        let paths = store::paths_for(&candidate)?;
        if !paths.dir.exists() {
            return Ok(candidate);
        }
    }
}

fn build_exec_args(cli: &codex_exec::Cli) -> Vec<String> {
    let mut args = Vec::new();

    for img in &cli.images {
        args.push("--image".into());
        args.push(img.to_string_lossy().into_owned());
    }

    if let Some(model) = &cli.model {
        args.push("--model".into());
        args.push(model.clone());
    }

    if cli.skip_git_repo_check {
        args.push("--skip-git-repo-check".into());
    }

    if cli.disable_response_storage {
        args.push("--disable-response-storage".into());
    }

    if let Some(prompt) = &cli.prompt {
        args.push(prompt.clone());
    }

    args
}

fn build_repl_args(cli: &codex_repl::Cli) -> Vec<String> {
    let mut args = Vec::new();

    // Positional prompt argument (optional) – needs to be *last* so push it later.

    if let Some(model) = &cli.model {
        args.push("--model".into());
        args.push(model.clone());
    }

    for img in &cli.images {
        args.push("--image".into());
        args.push(img.to_string_lossy().into_owned());
    }

    if cli.no_ansi {
        args.push("--no-ansi".into());
    }

    // Verbose flag is additive (-v -vv …).
    for _ in 0..cli.verbose {
        args.push("-v".into());
    }

    // Approval + sandbox policies
    args.push("--ask-for-approval".into());
    args.push(match cli.approval_policy {
        codex_core::ApprovalModeCliArg::OnFailure => "on-failure".into(),
        codex_core::ApprovalModeCliArg::UnlessAllowListed => "unless-allow-listed".into(),
        codex_core::ApprovalModeCliArg::Never => "never".into(),
    });

    args.push("--sandbox".into());
    args.push(match cli.sandbox_policy {
        codex_core::SandboxModeCliArg::NetworkRestricted => "network-restricted".into(),
        codex_core::SandboxModeCliArg::FileWriteRestricted => "file-write-restricted".into(),
        codex_core::SandboxModeCliArg::NetworkAndFileWriteRestricted => {
            "network-and-file-write-restricted".into()
        }
        codex_core::SandboxModeCliArg::DangerousNoRestrictions => {
            "dangerous-no-restrictions".into()
        }
    });

    if cli.allow_no_git_exec {
        args.push("--allow-no-git-exec".into());
    }

    if cli.disable_response_storage {
        args.push("--disable-response-storage".into());
    }

    if let Some(path) = &cli.record_submissions {
        args.push("--record-submissions".into());
        args.push(path.to_string_lossy().into_owned());
    }

    if let Some(path) = &cli.record_events {
        args.push("--record-events".into());
        args.push(path.to_string_lossy().into_owned());
    }

    // Finally positional prompt argument.
    if let Some(prompt) = &cli.prompt {
        args.push(prompt.clone());
    }

    args
}

// Build argument vector for spawning `codex-tui`.
// For the first implementation we forward only a minimal subset of options that
// are already handled in the REPL helper above.  Future work can extend this
// with the full flag surface.
fn build_tui_args(cli: &codex_tui::Cli) -> Vec<String> {
    let mut args = Vec::new();

    // Positional prompt argument (optional) – must be last.

    if let Some(model) = &cli.model {
        args.push("--model".into());
        args.push(model.clone());
    }

    for img in &cli.images {
        args.push("--image".into());
        args.push(img.to_string_lossy().into_owned());
    }

    if cli.skip_git_repo_check {
        args.push("--skip-git-repo-check".into());
    }

    if cli.disable_response_storage {
        args.push("--disable-response-storage".into());
    }

    // Approval + sandbox policies
    args.push("--ask-for-approval".into());
    args.push(match cli.approval_policy {
        codex_core::ApprovalModeCliArg::OnFailure => "on-failure".into(),
        codex_core::ApprovalModeCliArg::UnlessAllowListed => "unless-allow-listed".into(),
        codex_core::ApprovalModeCliArg::Never => "never".into(),
    });

    args.push("--sandbox".into());
    args.push(match cli.sandbox_policy {
        codex_core::SandboxModeCliArg::NetworkRestricted => "network-restricted".into(),
        codex_core::SandboxModeCliArg::FileWriteRestricted => "file-write-restricted".into(),
        codex_core::SandboxModeCliArg::NetworkAndFileWriteRestricted =>
            "network-and-file-write-restricted".into(),
        codex_core::SandboxModeCliArg::DangerousNoRestrictions =>
            "dangerous-no-restrictions".into(),
    });

    // Convenience flags
    if cli.full_auto {
        args.push("--full-auto".into());
    }
    if cli.suggest {
        args.push("--suggest".into());
    }

    if let Some(prompt) = &cli.prompt {
        args.push(prompt.clone());
    }

    args
}

// -----------------------------------------------------------------------------
// attach

#[derive(Args)]
pub struct AttachCmd {
    /// Session selector (index, id or prefix) to attach to.
    id: String,

    /// Also print stderr stream in addition to stdout.
    #[arg(long)]
    stderr: bool,
}

impl AttachCmd {
    pub async fn run(self) -> Result<()> {
        let id = store::resolve_selector(&self.id)?;
        let paths = store::paths_for(&id)?;

        // Load meta in order to decide which attach strategy to use.
        let meta_bytes = std::fs::read(&paths.meta)?;
        let meta: store::SessionMeta = serde_json::from_slice(&meta_bytes)?;

        match meta.kind {
            store::SessionKind::Exec | store::SessionKind::Repl => {
                self.attach_line_oriented(&id, &paths).await
            }
            store::SessionKind::Tui => {
                self.attach_tui(&paths).await
            }
        }
    }

    // ------------------------------------------------------------------
    // Original FIFO based attach (exec / repl)
    async fn attach_line_oriented(&self, id: &str, paths: &store::Paths) -> Result<()> {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncWriteExt;
        use tokio::time::{sleep, Duration};

        // Ensure stdin pipe exists.
        if !paths.stdin.exists() {
            anyhow::bail!("session '{id}' is not interactive (stdin pipe missing)");
        }

        // Open writer to the session's stdin pipe.
        let mut pipe = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&paths.stdin)
            .await
            .with_context(|| format!("failed to open stdin pipe for session '{id}'"))?;

        // Open stdout / stderr for tailing.
        let file_out = tokio::fs::File::open(&paths.stdout).await?;
        let mut reader_out = tokio::io::BufReader::new(file_out).lines();

        let mut stdin_lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();

        loop {
            tokio::select! {
                // User supplied input
                line = stdin_lines.next_line() => {
                    match line? {
                        Some(mut l) => {
                            l.push('\n');
                            pipe.write_all(l.as_bytes()).await?;
                            pipe.flush().await?;
                        }
                        None => {
                            // Ctrl-D
                            break;
                        }
                    }
                }
                // stdout updates
                out_line = reader_out.next_line() => {
                    match out_line? {
                        Some(l) => println!("{l}"),
                        None => sleep(Duration::from_millis(200)).await,
                    }
                }
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------------
    // TUI attach: raw byte forwarding over unix socket
    async fn attach_tui(&self, paths: &store::Paths) -> Result<()> {
        #[cfg(unix)]
        {
            use crossterm::{execute, terminal};
            use tokio::io;
            use tokio::net::UnixStream;

            // RAII guard for raw mode + alternate screen.
            struct TermGuard;
            impl TermGuard {
                fn new() -> anyhow::Result<Self> {
                    execute!(std::io::stdout(), terminal::EnterAlternateScreen)?;
                    terminal::enable_raw_mode()?;
                    Ok(Self)
                }
            }
            impl Drop for TermGuard {
                fn drop(&mut self) {
                    let _ = terminal::disable_raw_mode();
                    let _ = execute!(std::io::stdout(), terminal::LeaveAlternateScreen);
                }
            }

            let _term = TermGuard::new()?;

            let sock_path = paths.dir.join("sock");
            if !sock_path.exists() {
                anyhow::bail!(
                    "tui session socket not found ({}). Is the session fully initialised?",
                    sock_path.display()
                );
            }

            // Connect to session socket.
            let stream = UnixStream::connect(&sock_path).await?;
            let (mut reader, mut writer) = stream.into_split();

            let mut stdin = tokio::io::stdin();
            let mut stdout = tokio::io::stdout();

            let mut to_stdout = tokio::spawn(async move {
                let _ = io::copy(&mut reader, &mut stdout).await;
            });

            let mut to_socket = tokio::spawn(async move {
                let _ = io::copy(&mut stdin, &mut writer).await;
            });

            // Wait for either direction, then abort the other.
            tokio::select! {
                _ = &mut to_stdout => {
                    to_socket.abort();
                }
                _ = &mut to_socket => {
                    to_stdout.abort();
                }
            }

            let _ = to_stdout.await;
            let _ = to_socket.await;

            Ok(())
        }

        #[cfg(not(unix))]
        {
            anyhow::bail!("tui sessions are only supported on Unix at the moment");
        }
    }
}

// -----------------------------------------------------------------------------
// delete

#[derive(Args)]
pub struct DeleteCmd {
    id: String,
}

impl DeleteCmd {
    pub async fn run(self) -> Result<()> {
        let id = store::resolve_selector(&self.id)?;
        store::kill_session(&id).await?;
        store::purge(&id)?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// logs

#[derive(Args)]
pub struct LogsCmd {
    id: String,

    #[arg(short, long)]
    follow: bool,

    #[arg(long)]
    stderr: bool,
}

impl LogsCmd {
    pub async fn run(self) -> Result<()> {
        let id = store::resolve_selector(&self.id)?;
        let paths = store::paths_for(&id)?;
        let target = if self.stderr {
            &paths.stderr
        } else {
            &paths.stdout
        };

        let file = tokio::fs::File::open(target).await?;

        if self.follow {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(file).lines();
            while let Some(line) = lines.next_line().await? {
                println!("{line}");
            }
        } else {
            tokio::io::copy(
                &mut tokio::io::BufReader::new(file),
                &mut tokio::io::stdout(),
            )
            .await?;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// list

#[derive(Copy, Clone, ValueEnum, Debug)]
enum OutputFormat {
    Table,
    Json,
    Yaml,
}

#[derive(Args)]
pub struct ListCmd {
    /// Output format (default: table).
    #[arg(short = 'o', long = "output", value_enum, default_value_t = OutputFormat::Table)]
    output: OutputFormat,
}

#[derive(Serialize)]
#[allow(missing_docs)]
pub struct StatusRow {
    pub idx: usize,
    pub id: String,
    pub pid: u32,
    pub kind: String,
    pub status: String,
    pub created: String,
    pub prompt: String,
    pub out: String,
    pub err: String,
}

impl ListCmd {
    pub async fn run(self) -> Result<()> {
        use sysinfo::PidExt;
        use sysinfo::SystemExt;

        let metas = store::list_sessions_sorted()?;

        let mut sys = sysinfo::System::new();
        sys.refresh_processes();

        let rows: Vec<StatusRow> = metas
            .into_iter()
            .enumerate()
            .map(|(idx, m)| {
                let status = if m.pid == 0 {
                    "unknown"
                } else if sys.process(sysinfo::Pid::from_u32(m.pid)).is_some() {
                    "running"
                } else {
                    "exited"
                };

                let paths = store::paths_for(&m.id).ok();
                let (out, err) = if let Some(p) = &paths {
                    let osz = std::fs::metadata(&p.stdout).map(|m| m.len()).unwrap_or(0);
                    let esz = std::fs::metadata(&p.stderr).map(|m| m.len()).unwrap_or(0);
                    (human_bytes(osz), human_bytes(esz))
                } else {
                    ("-".into(), "-".into())
                };

                StatusRow {
                    idx,
                    id: m.id,
                    pid: m.pid,
                    kind: format!("{:?}", m.kind).to_lowercase(),
                    status: status.into(),
                    created: m.created_at.to_rfc3339(),
                    prompt: m.prompt_preview.unwrap_or_default(),
                    out,
                    err,
                }
            })
            .collect();

        match self.output {
            OutputFormat::Table => print_table(&rows)?,
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
            OutputFormat::Yaml => println!("{}", serde_yaml::to_string(&rows)?),
        }

        Ok(())
    }
}

pub fn print_table(rows: &[StatusRow]) -> Result<()> {
    use std::io::Write;
    use tabwriter::TabWriter;

    let mut tw = TabWriter::new(Vec::new()).padding(2);
    writeln!(tw, "#\tID\tPID\tTYPE\tSTATUS\tOUT\tERR\tCREATED\tPROMPT")?;
    for r in rows {
        writeln!(
            tw,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.idx, r.id, r.pid, r.kind, r.status, r.out, r.err, r.created, r.prompt
        )?;
    }
    let out = String::from_utf8(tw.into_inner()?)?;
    print!("{out}");
    Ok(())
}
