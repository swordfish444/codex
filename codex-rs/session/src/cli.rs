//! Command-line interface definition and dispatch.

use crate::{spawn, store};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};

/// Top-level CLI entry (re-exported by the crate).
#[derive(Parser)]
#[command(name = "codex-session", about = "Manage detached codex-exec sessions")]
pub struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

impl Cli {
    pub async fn dispatch(self) -> Result<()> {
        match self.cmd {
            Commands::Create(x) => x.run().await,
            Commands::Delete(x) => x.run().await,
            Commands::Logs(x) => x.run().await,
            Commands::Exec(x) => x.run().await,
            Commands::List(x) => x.run().await,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Spawn a new, detached agent.
    Create(CreateCmd),

    /// Kill a running session and delete on-disk artefacts.
    Delete(DeleteCmd),

    /// Show (and optionally follow) stdout / stderr logs of a session.
    Logs(LogsCmd),

    /// Execute a one-shot command inside an existing session.
    Exec(ExecCmd),

    /// List all known session IDs.
    List(ListCmd),
}

// -----------------------------------------------------------------------------
// create

#[derive(Args)]
pub struct CreateCmd {
    /// A custom session identifier. When omitted, a random UUIDv4 is used.
    #[arg(long)]
    id: Option<String>,

    /// Path to the `codex-exec` binary. Defaults to relying on $PATH.
    #[arg(long, default_value = "codex-exec")]
    exec: String,

    /// If set, terminate the agent when the CLI process exits ("attached" mode).
    #[arg(long)]
    kill_on_drop: bool,
}

impl CreateCmd {
    pub async fn run(self) -> Result<()> {
        let id = self
            .id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Persist basic metadata & directory skeleton *before* spawning the process.
        let meta = store::SessionMeta {
            id: id.clone(),
            created_at: chrono::Utc::now(),
        };

        let paths = store::paths_for(&id)?;
        store::materialise(&paths, &meta)?;

        // Spawn the background agent and immediately detach.
        let mut child = spawn::spawn_agent(&self.exec, &id, &paths, self.kill_on_drop)?;

        if self.kill_on_drop {
            // Hold the handle for the lifetime of the CLI; when we drop at the end of
            // `run()` the agent will be terminated by the `kill_on_drop` setting.
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
        }

        // When not in kill_on_drop mode we *immediately* drop the handle so the agent can
        // outlive us.
        println!("{id}");
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// delete

#[derive(Args)]
pub struct DeleteCmd {
    /// Session ID to terminate and remove.
    id: String,
}

impl DeleteCmd {
    pub async fn run(self) -> Result<()> {
        store::kill_session(&self.id).await?;
        store::purge(&self.id)?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// logs

#[derive(Args)]
pub struct LogsCmd {
    /// Session ID whose logs should be printed.
    id: String,

    /// Follow the file and stream appended lines (like `tail -f`).
    #[arg(short, long)]
    follow: bool,

    /// Show stderr instead of stdout.
    #[arg(long)]
    stderr: bool,
}

impl LogsCmd {
    pub async fn run(self) -> Result<()> {
        use tokio::io::AsyncBufReadExt;

        let paths = store::paths_for(&self.id)?;
        let target = if self.stderr {
            &paths.stderr
        } else {
            &paths.stdout
        };

        let file = tokio::fs::File::open(target).await?;

        if self.follow {
            let mut reader = tokio::io::BufReader::new(file);
            let mut lines = reader.lines();
            while let Some(line) = lines.next_line().await? {
                println!("{line}");
            }
        } else {
            // Simply dump the file contents to stdout.
            let mut stdout = tokio::io::stdout();
            tokio::io::copy(&mut tokio::io::BufReader::new(file), &mut stdout).await?;
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// exec (not implemented yet)

#[derive(Args)]
pub struct ExecCmd {
    id: String,

    /// Remaining arguments form the command to execute.
    #[arg(trailing_var_arg = true)]
    cmd: Vec<String>,
}

impl ExecCmd {
    pub async fn run(self) -> Result<()> {
        anyhow::bail!("exec inside an existing session is not yet implemented");
    }
}

// -----------------------------------------------------------------------------
// list

#[derive(Args)]
pub struct ListCmd;

impl ListCmd {
    pub async fn run(self) -> Result<()> {
        let sessions = store::list_sessions()?;
        for meta in sessions {
            println!("{}\t{}", meta.id, meta.created_at);
        }
        Ok(())
    }
}
