use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::Context;
use codex_common::ApprovalModeCliArg;
use codex_tui::Cli as TuiCli;

/// Attempt to handle a concurrent background run. Returns Ok(true) if a background exec
/// process was spawned (in which case the caller should NOT start the TUI), or Ok(false)
/// to proceed with normal interactive execution.
pub fn maybe_spawn_concurrent(
    tui_cli: &mut TuiCli,
    root_raw_overrides: &[String],
    concurrent: bool,
    concurrent_automerge: Option<bool>,
    concurrent_branch_name: &Option<String>,
) -> anyhow::Result<bool> {
    if !concurrent { return Ok(false); }

    // Enforce autonomous execution conditions when running interactive mode.
    // Validate git repository presence (required for --concurrent) only if we're in interactive path.
    {
        let dir_to_check = tui_cli
            .cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let status = Command::new("git")
            .arg("-C")
            .arg(&dir_to_check)
            .arg("rev-parse")
            .arg("--git-dir")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.as_ref().map(|s| !s.success()).unwrap_or(true) {
            eprintln!(
                "Error: --concurrent requires a git repository (directory {:?} is not managed by git).",
                dir_to_check
            );
            std::process::exit(2);
        }
    }

    let ap = tui_cli.approval_policy;
    let approval_on_failure = matches!(ap, Some(ApprovalModeCliArg::OnFailure));
    let autonomous = tui_cli.full_auto
        || tui_cli.dangerously_bypass_approvals_and_sandbox
        || approval_on_failure;
    if !autonomous {
        eprintln!(
            "Error: --concurrent requires autonomous mode. Use one of: --full-auto, --ask-for-approval on-failure, or --dangerously-bypass-approvals-and-sandbox."
        );
        std::process::exit(2);
    }
    if tui_cli.prompt.is_none() {
        eprintln!(
            "Error: --concurrent requires a prompt argument so the agent does not wait for interactive input."
        );
        std::process::exit(2);
    }

    // Build exec args from interactive CLI for autonomous run without TUI (background).
    let mut exec_args: Vec<String> = Vec::new();
    if !tui_cli.images.is_empty() {
        exec_args.push("--image".into());
        exec_args.push(tui_cli.images.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(","));
    }
    if let Some(model) = &tui_cli.model { exec_args.push("--model".into()); exec_args.push(model.clone()); }
    if let Some(profile) = &tui_cli.config_profile { exec_args.push("--profile".into()); exec_args.push(profile.clone()); }
    if let Some(sandbox) = &tui_cli.sandbox_mode { exec_args.push("--sandbox".into()); exec_args.push(format!("{sandbox:?}").to_lowercase().replace('_', "-")); }
    if tui_cli.full_auto { exec_args.push("--full-auto".into()); }
    if tui_cli.dangerously_bypass_approvals_and_sandbox { exec_args.push("--dangerously-bypass-approvals-and-sandbox".into()); }
    if tui_cli.skip_git_repo_check { exec_args.push("--skip-git-repo-check".into()); }
    for raw in root_raw_overrides { exec_args.push("-c".into()); exec_args.push(raw.clone()); }

    // Derive a single slug (shared by worktree branch & log filename) from the prompt.
    let raw_prompt = tui_cli.prompt.as_deref().unwrap_or("");
    let snippet = raw_prompt.chars().take(32).collect::<String>();
    let mut slug: String = snippet
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    while slug.contains("--") { slug = slug.replace("--", "-"); }
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { slug = "prompt".into(); }

    // Determine concurrent defaults from env (no config file), then apply CLI precedence.
    let env_automerge = parse_env_bool("CONCURRENT_AUTOMERGE");
    let env_branch_name = std::env::var("CONCURRENT_BRANCH_NAME").ok();
    let effective_automerge = concurrent_automerge.or(env_automerge).unwrap_or(true);
    let user_branch_name_opt = concurrent_branch_name.clone().or(env_branch_name);
    let branch_name_effective = if let Some(bn_raw) = user_branch_name_opt.as_ref() {
        let bn_trim = bn_raw.trim();
        if bn_trim.is_empty() { format!("codex/{slug}") } else { bn_trim.to_string() }
    } else {
        format!("codex/{slug}")
    };

    // Unique job id for this concurrent run (used for log file naming instead of slug).
    let job_id = uuid::Uuid::new_v4().to_string();

    // If user did NOT specify an explicit cwd, create an isolated git worktree.
    let mut created_worktree: Option<(PathBuf, String)> = None; // (path, branch)
    let mut original_branch: Option<String> = None;
    let mut original_commit: Option<String> = None;
    if tui_cli.cwd.is_none() {
        // Capture original branch & commit (best-effort).
        original_branch = git_capture(["rev-parse", "--abbrev-ref", "HEAD"]).ok();
        original_commit = git_capture(["rev-parse", "HEAD"]).ok();
        // Use branch_name_effective for branch/worktree name.
        match create_concurrent_worktree(&branch_name_effective) {
            Ok(Some((worktree_path, branch_name))) => {
                println!(
                    "Created git worktree at {} (branch {}) for concurrent run",
                    worktree_path.display(), branch_name
                );
                exec_args.push("--cd".into());
                exec_args.push(worktree_path.display().to_string());
                created_worktree = Some((worktree_path, branch_name));
            }
            Ok(None) => {
                eprintln!("Warning: Not a git repository (skipping worktree creation); running in current directory.");
            }
            Err(e) => {
                eprintln!("Error: failed to create git worktree for --concurrent: {e}");
                eprintln!("Hint: remove or rename existing branch '{branch_name_effective}', or pass --concurrent-branch-name to choose a unique name.");
                std::process::exit(3);
            }
        }
    } else if let Some(explicit) = &tui_cli.cwd {
        exec_args.push("--cd".into());
        exec_args.push(explicit.display().to_string());
    }

    // Prompt (safe to unwrap due to earlier validation).
    if let Some(prompt) = tui_cli.prompt.clone() { exec_args.push(prompt); }

    // Prepare log file path using stable job id (UUID) rather than prompt slug.
    let log_dir = match codex_base_dir() {
        Ok(base) => {
            let d = base.join("log");
            let _ = std::fs::create_dir_all(&d);
            d
        }
        Err(_) => PathBuf::from("/tmp"),
    };
    let log_path = log_dir.join(format!("codex-logs-{}.log", job_id));

    match File::create(&log_path) {
        Ok(file) => {
            let file_err = file.try_clone().ok();
            let mut cmd = Command::new(
                std::env::current_exe().unwrap_or_else(|_| PathBuf::from("codex"))
            );
            cmd.arg("exec");
            for a in &exec_args { cmd.arg(a); }
            // Provide metadata for auto merge if we created a worktree.
            if let Some((wt_path, branch)) = &created_worktree {
                if effective_automerge { cmd.env("CODEX_CONCURRENT_AUTOMERGE", "1"); }
                cmd.env("CODEX_CONCURRENT_BRANCH", branch);
                cmd.env("CODEX_CONCURRENT_WORKTREE", wt_path);
                if let Some(ob) = &original_branch { cmd.env("CODEX_ORIGINAL_BRANCH", ob); }
                if let Some(oc) = &original_commit { cmd.env("CODEX_ORIGINAL_COMMIT", oc); }
                if let Ok(orig_root) = std::env::current_dir() { cmd.env("CODEX_ORIGINAL_ROOT", orig_root); }
            }
            // Provide job id so child process can emit token_count updates to tasks.jsonl.
            cmd.env("CODEX_JOB_ID", &job_id);
            cmd.stdout(Stdio::from(file));
            if let Some(f2) = file_err { cmd.stderr(Stdio::from(f2)); }
            match cmd.spawn() {
                Ok(child) => {
                    if let Some((wt_path, wt_branch)) = &created_worktree {
                        println!(
                            "Background Codex exec started in worktree. PID={} job_id={} log={} worktree={} branch={} original_branch={} automerge={}",
                            child.id(), job_id, log_path.display(), wt_path.display(), wt_branch,
                            original_branch.as_deref().unwrap_or("?"), effective_automerge
                        );
                    } else {
                        println!(
                            "Background Codex exec started. PID={} job_id={} log={} automerge={}",
                            child.id(), job_id, log_path.display(), effective_automerge
                        );
                    }

                    // Record job metadata to CODEX_HOME/jobs.jsonl (JSON Lines file).
                    let record_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if let Ok(base) = codex_base_dir() {
                        let tasks_path = base.join("tasks.jsonl");
                        let record = serde_json::json!({
                            "job_id": job_id,
                            "pid": child.id(),
                            "worktree": created_worktree.as_ref().map(|(p, _)| p.display().to_string()),
                            "branch": created_worktree.as_ref().map(|(_, b)| b.clone()),
                            "original_branch": original_branch,
                            "original_commit": original_commit,
                            "log_path": log_path.display().to_string(),
                            "prompt": raw_prompt,
                            "model": tui_cli.model.clone(),
                            "start_time": record_time,
                            "automerge": effective_automerge,
                            "explicit_branch_name": user_branch_name_opt,
                            "token_count": serde_json::Value::Null,
                            "state": "started",
                        });
                        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&tasks_path) {
                            use std::io::Write;
                            if let Err(e) = writeln!(f, "{}", record.to_string()) {
                                eprintln!("Warning: failed writing task record to {}: {e}", tasks_path.display());
                            }
                        } else {
                            eprintln!("Warning: could not open tasks log file at {}", tasks_path.display());
                        }
                    }
                    return Ok(true); // background spawned
                }
                Err(e) => {
                    eprintln!("Failed to start background exec: {e}. Falling back to interactive mode.");
                }
            }
        }
        Err(e) => {
            eprintln!(
                "Failed to create log file {}: {e}. Falling back to interactive mode.",
                log_path.display()
            );
        }
    }

    Ok(false)
}

/// Return the base Codex directory under the user's home (~/.codex), creating it if necessary.
fn codex_base_dir() -> anyhow::Result<PathBuf> {
    if let Ok(val) = std::env::var("CODEX_HOME") {
        if !val.is_empty() {
            return Ok(PathBuf::from(val).canonicalize()?);
        }
    }
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let base = PathBuf::from(home).join(".codex");
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Attempt to create a git worktree for an isolated concurrent run.
/// Returns Ok(Some((worktree_path, branch_name))) on success, Ok(None) if not a git repo, and Err on failure.
fn create_concurrent_worktree(branch_name: &str) -> anyhow::Result<Option<(PathBuf, String)>> {
    // Determine repository root.
    let output = Command::new("git").arg("rev-parse").arg("--show-toplevel").output();
    let repo_root = match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() { return Ok(None); }
            PathBuf::from(s)
        }
        _ => return Ok(None),
    };

    // Derive repo name from root directory.
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo");

    // Fast-fail if branch already exists.
    if Command::new("git")
        .current_dir(&repo_root)
        .arg("rev-parse")
        .arg("--verify")
        .arg(branch_name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false) {
        anyhow::bail!("branch '{branch_name}' already exists");
    }

    // Construct worktree directory under ~/.codex/worktrees/<repo_name>/.
    let base_dir = codex_base_dir()?.join("worktrees").join(repo_name);
    std::fs::create_dir_all(&base_dir)?;
    let mut worktree_path = base_dir.join(branch_name.replace('/', "-"));

    // Ensure uniqueness if path already exists.
    if worktree_path.exists() {
        for i in 1..1000 { // arbitrary cap
            let candidate = base_dir.join(format!("{}-{}", branch_name.replace('/', "-"), i));
            if !candidate.exists() { worktree_path = candidate; break; }
        }
    }

    // Run: git worktree add -b <branch_name> <path> HEAD
    let status = Command::new("git")
        .current_dir(&repo_root)
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg(&branch_name)
        .arg(&worktree_path)
        .arg("HEAD")
        .status()?;

    if !status.success() {
        anyhow::bail!("git worktree add failed with status {status}");
    }

    Ok(Some((worktree_path, branch_name.to_string())))
}

/// Helper: capture trimmed stdout of a git command.
fn git_capture<I, S>(args: I) -> anyhow::Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut cmd = Command::new("git");
    for a in args { cmd.arg(a.as_ref()); }
    let out = cmd.output().context("running git command")?;
    if !out.status.success() { anyhow::bail!("git command failed"); }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Parse common boolean environment variable representations.
fn parse_env_bool(name: &str) -> Option<bool> {
    let raw = std::env::var(name).ok()?;
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
} 