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
    /// Session identifier.  Generates a random UUIDv4 when omitted.
    #[arg(long)]
    id: Option<String>,

    /// All flags following `create` are forwarded to `codex-exec`.
    #[clap(flatten)]
    exec_cli: codex_exec::Cli,
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

        // Convert exec_cli back into a Vec<String> so we can forward them verbatim.
        let exec_args = build_exec_args(&self.exec_cli);

        // Spawn the background agent and immediately detach – we never hold on to the
        // Child handle.
        let _child = spawn::spawn_agent(&paths, &exec_args)?;
        println!("{id}");
        Ok(())
    }
}

/// Re-serialize a `codex_exec::Cli` struct back into the exact CLI args.
fn build_exec_args(cli: &codex_exec::Cli) -> Vec<String> {
    let mut args = Vec::new();

    for path in &cli.images {
        args.push("--image".to_string());
        args.push(path.to_string_lossy().into_owned());
    }

    if let Some(model) = &cli.model {
        args.push("--model".to_string());
        args.push(model.clone());
    }

    if cli.skip_git_repo_check {
        args.push("--skip-git-repo-check".to_string());
    }

    if cli.disable_response_storage {
        args.push("--disable-response-storage".to_string());
    }

    if let Some(prompt) = &cli.prompt {
        args.push(prompt.clone());
    }

    args
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
            let reader = tokio::io::BufReader::new(file);
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
