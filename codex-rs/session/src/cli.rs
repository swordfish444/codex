//! CLI command definitions and implementation.

use crate::{spawn, store};
use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

#[derive(Parser)]
#[command(name = "codex-session", about = "Manage codex-exec background sessions")]
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

fn human_bytes(b: u64) -> String {
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

#[derive(Subcommand)]
enum Commands {
    Create(CreateCmd),
    Delete(DeleteCmd),
    Logs(LogsCmd),
    Exec(ExecCmd),
    List(ListCmd),
}

// -----------------------------------------------------------------------------
// create

#[derive(Args)]
pub struct CreateCmd {
    /// Explicit session name. If omitted, a memorable random one is generated.
    #[arg(long)]
    id: Option<String>,

    /// Flags passed through to codex-exec.
    #[clap(flatten)]
    exec_cli: codex_exec::Cli,
}

impl CreateCmd {
    pub async fn run(self) -> Result<()> {
        let id = match self.id {
            Some(id) => id,
            None => generate_session_id()?,
        };

        let paths = store::paths_for(&id)?;
        store::prepare_dirs(&paths)?;

        let exec_args = build_exec_args(&self.exec_cli);

        // Preview first 40 printable chars of prompt for status listing
        let prompt_preview = self
            .exec_cli
            .prompt
            .as_ref()
            .map(|p| {
                let slice: String = p.chars().take(40).collect();
                if p.len() > 40 {
                    format!("{}…", slice)
                } else {
                    slice
                }
            });

        // Spawn process
        let child = spawn::spawn_agent(&paths, &exec_args)?;

        let meta = store::SessionMeta {
            id: id.clone(),
            pid: child.id().unwrap_or_default(),
            created_at: chrono::Utc::now(),
            prompt_preview,
        };
        store::write_meta(&paths, &meta)?;

        println!("{id}");
        Ok(())
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
        let target = if self.stderr { &paths.stderr } else { &paths.stdout };

        let file = tokio::fs::File::open(target).await?;

        if self.follow {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(file).lines();
            while let Some(line) = lines.next_line().await? {
                println!("{line}");
            }
        } else {
            tokio::io::copy(&mut tokio::io::BufReader::new(file), &mut tokio::io::stdout()).await?;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// exec (TODO)

#[derive(Args)]
pub struct ExecCmd {
    id: String,
    #[arg(trailing_var_arg = true)]
    cmd: Vec<String>,
}

impl ExecCmd {
    pub async fn run(self) -> Result<()> {
        let _id = store::resolve_selector(&self.id)?;
        anyhow::bail!("exec inside session not implemented yet");
    }
}

// -----------------------------------------------------------------------------
// list

#[derive(Copy, Clone, ValueEnum, Debug)]
enum OutputFormat { Table, Json, Yaml }

#[derive(Args)]
pub struct ListCmd {
    #[arg(short = 'o', long = "output", value_enum, default_value_t = OutputFormat::Table)]
    output: OutputFormat,
}

#[derive(Serialize)]
struct StatusRow {
    idx: usize,
    id: String,
    pid: u32,
    status: String,
    created: String,
    prompt: String,
    out: String,
    err: String,
}

impl ListCmd {
    pub async fn run(self) -> Result<()> {
        use sysinfo::{SystemExt, PidExt};

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

                // file sizes
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

fn print_table(rows: &[StatusRow]) -> Result<()> {
    use std::io::Write;
    use tabwriter::TabWriter;

    let mut tw = TabWriter::new(Vec::new()).padding(2);
    writeln!(tw, "#\tID\tPID\tSTATUS\tOUT\tERR\tCREATED\tPROMPT")?;
    for r in rows {
        writeln!(tw, "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}", r.idx, r.id, r.pid, r.status, r.out, r.err, r.created, r.prompt)?;
    }
    let out = String::from_utf8(tw.into_inner()?)?;
    print!("{out}");
    Ok(())
}
