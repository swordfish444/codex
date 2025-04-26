//! CLI command definitions and implementation for `codex-session`.
//!
//! The session manager can spawn two different Codex agent flavors:
//!
//! * `codex-exec` – non-interactive batch agent (legacy behaviour)
//! * `codex-repl` – interactive REPL that requires user input after launch
//!
//! The `create` command therefore has mutually exclusive sub-commands so the appropriate
//! arguments can be forwarded to the underlying agent binaries.

use crate::meta::SessionMeta;
use crate::spawn;
use crate::store;
use anyhow::Context;
use anyhow::Result;
use chrono::SecondsFormat;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;

// -----------------------------------------------------------------------------
// Platform-specific imports

#[cfg(unix)]
use codex_repl as _; // Ensures the dependency is only required on Unix.
#[allow(unused_imports)]
use serde::Serialize; // still needed for table print rows in tests

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
    // (previous mux variant removed)
}

// -----------------------------------------------------------------------------
// create

#[derive(Subcommand)]
enum AgentKind {
    /// Non-interactive execution agent.
    Exec(ExecCreateCmd),

    /// Interactive Read-Eval-Print-Loop agent.
    #[cfg(unix)]
    Repl(ReplCreateCmd),
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

#[cfg(unix)]
#[derive(Args)]
pub struct ReplCreateCmd {
    #[clap(flatten)]
    repl_cli: codex_repl::Cli,
}

impl CreateCmd {
    pub async fn run(self) -> Result<()> {
        let id = match &self.id {
            Some(explicit) => explicit.clone(),
            None => generate_session_id()?,
        };

        let paths = store::paths_for(&id)?;
        // -----------------------------------------------------------------
        // Prepare session directory *before* spawning the agent so stdout/
        // stderr redirection works even when the child process itself fails
        // immediately.
        // -----------------------------------------------------------------

        store::prepare_dirs(&paths)?;

        // -----------------------------------------------------------------
        // Spawn underlying agent.
        //
        // IMPORTANT: If the spawn call fails we end up with an empty (or
        // almost empty) directory inside ~/.codex/sessions/.  To avoid
        // confusing stale entries we attempt to purge the directory before
        // bubbling up the error to the caller.
        // -----------------------------------------------------------------

        // Capture the child PID *and* the full CLI config so we can persist it
        // in the metadata file.
        let spawn_result: Result<(
            u32,                // pid
            Option<String>,     // prompt preview
            store::SessionKind, // kind
            Vec<String>,        // raw argv used to spawn the agent
        )> = (|| match self.agent {
            AgentKind::Exec(cmd) => {
                let args = build_exec_args(&cmd.exec_cli);
                let child = spawn::spawn_exec(&paths, &args)?;

                let preview = cmd.exec_cli.prompt.as_ref().map(|p| truncate_preview(p));

                Ok((
                    child.id().unwrap_or_default(),
                    preview,
                    store::SessionKind::Exec,
                    args.clone(),
                ))
            }
            #[cfg(unix)]
            AgentKind::Repl(cmd) => {
                let args = build_repl_args(&cmd.repl_cli);
                let child = spawn::spawn_repl(&paths, &args)?;

                let preview = cmd.repl_cli.prompt.as_ref().map(|p| truncate_preview(p));

                Ok((
                    child.id().unwrap_or_default(),
                    preview,
                    store::SessionKind::Repl,
                    args.clone(),
                ))
            }
        })();

        let (pid, prompt_preview, kind, argv) = match spawn_result {
            Ok(tuple) => tuple,
            Err(err) => {
                // Best effort clean-up – ignore failures so we don't mask the
                // original spawn error.
                let _ = store::purge(&id);
                return Err(err);
            }
        };

        // Persist metadata **after** the process has been spawned so we can record its PID.
        // Persist metadata **after** the process has been spawned so we can record its PID.
        let meta = SessionMeta::new(id.clone(), pid, kind, argv, prompt_preview);

        store::write_meta(&paths, &meta)?;

        println!("{id}");
        Ok(())
    }
}

// (mux helper removed)

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

#[cfg(unix)]
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

        self.attach_line_oriented(&id, &paths).await
    }

    // ------------------------------------------------------------------
    // Original FIFO based attach (exec / repl)
    async fn attach_line_oriented(&self, id: &str, paths: &store::Paths) -> Result<()> {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncWriteExt;
        use tokio::time::sleep;
        use tokio::time::Duration;

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

        // ------------------------------------------------------------------
        // Log tailing setup
        //
        // The original implementation always tailed *stdout* only.  Honour the
        // `--stderr` flag so users can observe an interactive agent’s error
        // stream as well.  When the flag is **not** supplied we keep the
        // previous behaviour for backwards-compatibility.

        // Always open stdout so the select! branches below stay simple.
        let file_out = tokio::fs::File::open(&paths.stdout).await?;
        let mut reader_out = tokio::io::BufReader::new(file_out).lines();

        // Conditionally open stderr if the user asked for it.  Keeping the
        // reader in an `Option` allows us to reuse the same select! loop – the
        // helper future simply parks forever when stderr is disabled.
        let mut reader_err = if self.stderr {
            let file_err = tokio::fs::File::open(&paths.stderr).await?;
            Some(tokio::io::BufReader::new(file_err).lines())
        } else {
            None
        };

        let mut stdin_lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();

        loop {
            tokio::select! {
                // ------------------------------------------------------------------
                // User supplied input (stdin → session stdin pipe)
                line = stdin_lines.next_line() => {
                    match line? {
                        Some(mut l) => {
                            l.push('\n');
                            pipe.write_all(l.as_bytes()).await?;
                            pipe.flush().await?;
                        }
                        None => {
                            // Ctrl-D – end of interactive input
                            break;
                        }
                    }
                }

                // ------------------------------------------------------------------
                // stdout updates
                out_line = reader_out.next_line() => {
                    match out_line? {
                        Some(l) => println!("{l}"),
                        None => sleep(Duration::from_millis(200)).await,
                    }
                }

                // ------------------------------------------------------------------
                // stderr updates (optional)
                //
                // To keep `tokio::select!` happy we always supply a branch – when the
                // user did *not* request stderr we hand it a future that will never
                // finish (pending forever).  This avoids `Option` juggling within the
                // select! macro.
                err_line = async {
                    if let Some(reader) = &mut reader_err {
                        reader.next_line().await
                    } else {
                        // Never resolves – equivalent to `futures::future::pending()`
                        std::future::pending().await
                    }
                } => {
                    if let Some(line) = err_line? {
                        // Use a visible prefix so users can distinguish the streams.
                        println!("[stderr] {line}");
                    } else {
                        sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        }

        Ok(())
    }

    // (TUI attach removed)
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

        // Stream the complete file to stdout.  Users can pipe to `tail -f`,
        // `less +F`, etc. if they only want live updates.
        tokio::io::copy(
            &mut tokio::io::BufReader::new(file),
            &mut tokio::io::stdout(),
        )
        .await?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// list – newest-first overview of all sessions

#[derive(Args)]
pub struct ListCmd {}

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
                    created: m.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
                    prompt: m.prompt_preview.unwrap_or_default(),
                    out,
                    err,
                }
            })
            .collect();

        print_table(&rows)?;

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
