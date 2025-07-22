use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::fs;

#[derive(Debug, Parser)]
pub struct JobsCli {
    #[command(subcommand)]
    pub cmd: JobsCommand,
}

#[derive(Debug, Subcommand)]
pub enum JobsCommand {
    /// List background concurrent jobs (from ~/.codex/tasks.jsonl)
    Ls(JobsListArgs),
}

#[derive(Debug, Parser)]
pub struct JobsListArgs {
    /// Output raw JSON instead of table
    #[arg(long)]
    pub json: bool,
    /// Limit number of jobs displayed (most recent first)
    #[arg(long)]
    pub limit: Option<usize>,
    /// Show completed jobs as well (by default only running jobs)
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    /// Show all columns including prompt text
    #[arg(long = "all-columns")]
    pub all_columns: bool,
}

#[derive(Debug, Deserialize)]
struct RawRecord {
    job_id: Option<String>,
    pid: Option<u64>,
    worktree: Option<String>,
    branch: Option<String>,
    original_branch: Option<String>,
    original_commit: Option<String>,
    log_path: Option<String>,
    prompt: Option<String>,
    model: Option<String>,
    start_time: Option<u64>,
    update_time: Option<u64>,
    token_count: Option<serde_json::Value>,
    state: Option<String>,
    completion_time: Option<u64>,
    end_time: Option<u64>,
}

#[derive(Debug, Serialize, Default, Clone)]
struct JobAggregate {
    job_id: String,
    pid: Option<u64>,
    branch: Option<String>,
    worktree: Option<String>,
    prompt: Option<String>,
    model: Option<String>,
    start_time: Option<u64>,
    last_update_time: Option<u64>,
    total_tokens: Option<u64>,
    state: Option<String>,
    end_time: Option<u64>,
}

pub fn run_jobs(cmd: JobsCli) -> anyhow::Result<()> {
    match cmd.cmd {
        JobsCommand::Ls(args) => list_jobs(args),
    }
}

fn base_dir() -> Option<std::path::PathBuf> {
    if let Ok(val) = std::env::var("CODEX_HOME") { if !val.is_empty() { return std::fs::canonicalize(val).ok(); } }
    let home = std::env::var_os("HOME")?;
    let base = std::path::PathBuf::from(home).join(".codex");
    Some(base)
}

fn list_jobs(args: JobsListArgs) -> anyhow::Result<()> {
    let Some(base) = base_dir() else {
        println!("No home directory found; cannot locate tasks.jsonl");
        return Ok(());
    };
    let path = base.join("tasks.jsonl");
    if !path.exists() {
        println!("No tasks.jsonl found (no concurrent jobs recorded yet)");
        return Ok(());
    }

    let f = File::open(&path)?;
    let reader = BufReader::new(f);

    let mut agg: HashMap<String, JobAggregate> = HashMap::new();
    for line_res in reader.lines() {
        let line = match line_res { Ok(l) => l, Err(_) => continue };
        if line.trim().is_empty() { continue; }
        let raw: serde_json::Value = match serde_json::from_str(&line) { Ok(v) => v, Err(_) => continue };
        let rec: RawRecord = match serde_json::from_value(raw) { Ok(r) => r, Err(_) => continue };
        let Some(job_id) = rec.job_id.clone() else { continue }; // must have job_id
        let entry = agg.entry(job_id.clone()).or_insert_with(|| JobAggregate { job_id: job_id.clone(), ..Default::default() });
        if rec.start_time.is_some() { // initial metadata line
            entry.pid = rec.pid.or(entry.pid);
            entry.branch = rec.branch.or(entry.branch.clone());
            entry.worktree = rec.worktree.or(entry.worktree.clone());
            entry.prompt = rec.prompt.or(entry.prompt.clone());
            entry.model = rec.model.or(entry.model.clone());
            entry.start_time = rec.start_time.or(entry.start_time);
        }
        if let Some(tc_val) = rec.token_count.as_ref() { if tc_val.is_object() { if let Some(total) = tc_val.get("total_tokens").and_then(|v| v.as_u64()) { entry.total_tokens = Some(total); } } }
        if rec.update_time.is_some() { entry.last_update_time = rec.update_time; }
        if let Some(state) = rec.state { entry.state = Some(state); }
        if rec.completion_time.is_some() || rec.end_time.is_some() {
            entry.end_time = rec.end_time.or(rec.completion_time).or(entry.end_time);
        }
    }

    // Collect and sort by start_time desc
    let mut jobs: Vec<JobAggregate> = agg.into_values().collect();
    jobs.sort_by_key(|j| std::cmp::Reverse(j.start_time.unwrap_or(0)));

    if !args.all { jobs.retain(|j| j.state.as_deref() != Some("done")); }
    if let Some(limit) = args.limit { jobs.truncate(limit); }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&jobs)?);
        return Ok(());
    }

    if jobs.is_empty() {
        println!("No jobs found");
        return Ok(());
    }

    // Table header
    if args.all_columns {
        println!("{:<8} {:>6} {:<22} {:<12} {:<8} {:>8} {:<12} {}", "JOB_ID", "PID", "BRANCH", "START", "STATE", "TOKENS", "MODEL", "PROMPT");
    } else {
        // Widened branch column to 22 chars for better readability.
        println!("{:<8} {:>6} {:<22} {:<12} {:<8} {:>8} {:<12}", "JOB_ID", "PID", "BRANCH", "START", "STATE", "TOKENS", "MODEL");
    }
    for j in jobs {
        let job_short = if j.job_id.len() > 8 { &j.job_id[..8] } else { &j.job_id };
        let pid_str = j.pid.map(|p| p.to_string()).unwrap_or_default();
        let mut branch = j.branch.clone().unwrap_or_default();
        let branch_limit = if args.all_columns { 22 } else { 22 }; // unified width
        if branch.len() > branch_limit { branch.truncate(branch_limit); }
        let start = j.start_time.map(format_epoch_short).unwrap_or_default();
        let tokens = j.total_tokens.map(|t| t.to_string()).unwrap_or_default();
        let state = j.state.clone().unwrap_or_else(|| "?".into());
        let mut model = j.model.clone().unwrap_or_default();
        if model.trim().is_empty() { model = resolve_default_model(); }
        if model.is_empty() { model.push('-'); }
        if model.len() > 12 { model.truncate(12); }
        if args.all_columns {
            let mut prompt = j.prompt.clone().unwrap_or_default().replace('\n', " ");
            if prompt.len() > 60 { prompt.truncate(60); }
            println!("{:<8} {:>6} {:<22} {:<12} {:<8} {:>8} {:<12} {}", job_short, pid_str, branch, start, state, tokens, model, prompt);
        } else {
            println!("{:<8} {:>6} {:<22} {:<12} {:<8} {:>8} {:<12}", job_short, pid_str, branch, start, state, tokens, model);
        }
    }

    Ok(())
}

fn format_epoch_short(secs: u64) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let dt = Local.timestamp_opt(secs as i64, 0).single();
    if let Some(dt) = dt {
        let now = Local::now();
        if dt.year() == now.year() {
            dt.format("%d %b %H:%M").to_string() // e.g. 22 Jul 11:56
        } else {
            dt.format("%d %b %Y").to_string() // older year
        }
    } else {
        String::new()
    }
}

fn resolve_default_model() -> String {
    // Attempt to read config json/yaml for model, otherwise fallback to hardcoded default.
    if let Some(base) = base_dir() {
        let candidates = ["config.json", "config.yaml", "config.yml"];
        for name in candidates {
            let p = base.join(name);
            if p.exists() {
                if let Ok(raw) = fs::read_to_string(&p) {
                    // Try JSON first.
                    if name.ends_with(".json") {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                            if let Some(m) = v.get("model").and_then(|x| x.as_str()) {
                                if !m.trim().is_empty() { return m.to_string(); }
                            }
                        }
                    } else {
                        // Very lightweight YAML parse: look for line starting with model:
                        for line in raw.lines() {
                            if let Some(rest) = line.trim().strip_prefix("model:") {
                                let val = rest.trim().trim_matches('"');
                                if !val.is_empty() {
                                    return val.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Fallback default agentic model used elsewhere.
    "codex-mini-latest".to_string()
} 