//! Session bookkeeping helpers.
//!
//! A session lives in `~/.codex/sessions/<id>/` and contains:
//! * stdout.log / stderr.log       - redirect of agent io
//! * meta.json                     - small struct saved by `write_meta`.

use anyhow::Context;
use anyhow::Result;

// The rich metadata envelope lives in its own module so other parts of the
// crate can import it without pulling in the whole `store` implementation.
use crate::meta::SessionMeta;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub dir: PathBuf,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    /// Named pipe used for interactive stdin when the session runs a `codex-repl` agent.
    ///
    /// The file is **only** created for repl sessions.  Exec sessions ignore the path.
    pub stdin: PathBuf,
    pub meta: PathBuf,
}

/// Calculate canonical paths for the given session ID.
/// Build a [`Paths`] struct for a given session identifier.
///
/// The function validates the input to avoid path-traversal attacks or
/// accidental creation of nested directories.  Only the following ASCII
/// characters are accepted:
///
/// * `A-Z`, `a-z`, `0-9`
/// * underscore (`_`)
/// * hyphen (`-`)
///
/// Any other byte -- especially path separators such as `/` or `\\` -- results
/// in an error.
///
/// Keeping the validation local to this helper ensures that *all* call-sites
/// (CLI, library, tests) get the same guarantees.
pub fn paths_for(id: &str) -> Result<Paths> {
    validate_id(id)?;

    // No IO here. Only build the paths.
    let dir = base_dir()?.join(id);
    Ok(Paths {
        dir: dir.clone(),
        stdout: dir.join("stdout.log"),
        stderr: dir.join("stderr.log"),
        stdin: dir.join("stdin.pipe"),
        meta: dir.join("meta.json"),
    })
}

/// Internal helper: ensure the supplied session id is well-formed.
fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() {
        anyhow::bail!("session id must not be empty");
    }

    for b in id.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' => {}
            _ => anyhow::bail!("invalid character in session id: {:?}", b as char),
        }
    }

    Ok(())
}

fn base_dir() -> Result<PathBuf> {
    // ~/.codex/sessions
    let home = dirs::home_dir().context("could not resolve home directory")?;
    Ok(home.join(".codex").join("sessions"))
}

// Keep the original `SessionKind` enum here so we don't need a breaking change
// in all call-sites.  The enum is re-exported so other modules (e.g. the newly
// added `meta` module) can still rely on the single source of truth.

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    /// Non-interactive batch session -- `codex-exec`.
    Exec,
    /// Line-oriented interactive session -- `codex-repl`.
    Repl,
}

impl Default for SessionKind {
    fn default() -> Self {
        SessionKind::Exec
    }
}

/// Create the on-disk directory structure and write metadata + empty log files.
/// Create directory & empty log files. Does **not** write metadata; caller should write that
/// once the child process has actually been spawned so we can record its PID.
pub fn prepare_dirs(paths: &Paths) -> Result<()> {
    // Called before spawn to make sure log files already exist.
    std::fs::create_dir_all(&paths.dir)?;

    for p in [&paths.stdout, &paths.stderr] {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)?;
    }
    Ok(())
}

pub fn write_meta(paths: &Paths, meta: &SessionMeta) -> Result<()> {
    // Persist metadata after successful spawn so we can record PID.
    std::fs::write(&paths.meta, serde_json::to_vec_pretty(meta)?)?;
    Ok(())
}

/// Enumerate all sessions by loading each `meta.json`.
pub fn list_sessions() -> Result<Vec<SessionMeta>> {
    let mut res = Vec::new();
    let base = base_dir()?;
    if base.exists() {
        for entry in std::fs::read_dir(base)? {
            let entry = entry?;
            let meta_path = entry.path().join("meta.json");
            if let Ok(bytes) = std::fs::read(&meta_path) {
                if let Ok(meta) = serde_json::from_slice::<SessionMeta>(&bytes) {
                    res.push(meta);
                }
            }
        }
    }
    Ok(res)
}

/// List sessions sorted by newest first (created_at desc).
/// Newest-first list (created_at descending).
pub fn list_sessions_sorted() -> Result<Vec<SessionMeta>> {
    let mut v = list_sessions()?;
    v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(v)
}

/// Resolve a user-supplied selector to a concrete session id.
///
/// Rules:
/// 1. Pure integer ⇒ index into newest-first list (0 = most recent)
/// 2. Otherwise try exact id match, then unique prefix match.
pub fn resolve_selector(sel: &str) -> Result<String> {
    // Accept index, full id, or unique prefix.
    let list = list_sessions_sorted()?;

    // numeric index
    if let Ok(idx) = sel.parse::<usize>() {
        return list
            .get(idx)
            .map(|m| m.id.clone())
            .context(format!("no session at index {idx}"));
    }

    // exact match
    if let Some(m) = list.iter().find(|m| m.id == sel) {
        return Ok(m.id.clone());
    }

    // unique prefix match
    let mut matches: Vec<&SessionMeta> = list.iter().filter(|m| m.id.starts_with(sel)).collect();
    match matches.len() {
        1 => Ok(matches.remove(0).id.clone()),
        0 => anyhow::bail!("no session matching '{sel}'"),
        _ => anyhow::bail!("selector '{sel}' is ambiguous ({} matches)", matches.len()),
    }
}

/// Send a polite termination request to the session’s process.
///
/// NOTE: Full PID accounting is a future improvement; for now the function
/// simply returns `Ok(())` so the `delete` command doesn’t fail.
/// Attempt to terminate the process (group) that belongs to the given session id.
///
/// Behaviour
/// 1. A *graceful* `SIGTERM` (or `CTRL-BREAK` on Windows) is sent to the **process group**
///    that was created when the agent was spawned (`setsid` / `CREATE_NEW_PROCESS_GROUP`).
/// 2. We wait for a short grace period so the process can exit cleanly.
/// 3. If the process (identified by the original PID) is still alive we force-kill it
///    with `SIGKILL` (or the Win32 `TerminateProcess` API).
/// 4. The function is **idempotent** -- calling it again when the session is already
///    terminated returns an error (`Err(AlreadyDead)`) so callers can decide whether
///    they still need to clean up the directory (`store::purge`).
///
/// NOTE: only a very small amount of asynchronous work is required (the sleeps between
/// TERM → KILL).  We keep the function `async` so the public signature stays unchanged.
pub async fn kill_session(id: &str) -> Result<()> {
    use std::time::Duration;

    // Resolve paths and read metadata so we know the target PID.
    let paths = paths_for(id)?;

    // Load meta.json -- we need the PID written at spawn time.
    let bytes = std::fs::read(&paths.meta)
        .with_context(|| format!("could not read metadata for session '{id}'"))?;
    let meta: SessionMeta =
        serde_json::from_slice(&bytes).context("failed to deserialize session metadata")?;

    let pid_u32 = meta.pid;

    // Helper -- cross-platform liveness probe based on the `sysinfo` crate.
    fn is_alive(pid: u32) -> bool {
        use sysinfo::PidExt;
        use sysinfo::SystemExt;

        let mut sys = sysinfo::System::new();
        sys.refresh_process(sysinfo::Pid::from_u32(pid));
        sys.process(sysinfo::Pid::from_u32(pid)).is_some()
    }

    // If the process is already gone we bail out so the caller knows the session
    // directory might need manual clean-up.
    let mut still_running = is_alive(pid_u32);

    if !still_running {
        anyhow::bail!(
            "session process (PID {pid_u32}) is not running -- directory cleanup still required"
        );
    }

    //---------------------------------------------------------------------
    // Step 1 -- send graceful termination.
    //---------------------------------------------------------------------

    #[cfg(unix)]
    {
        // Negative PID = process-group.
        let pgid = -(pid_u32 as i32);
        unsafe {
            libc::kill(pgid, libc::SIGTERM);
        }
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent;
        const CTRL_BREAK_EVENT: u32 = 1; // Using BREAK instead of C for detached groups.
                                         // The process group id on Windows *is* the pid that we passed to CREATE_NEW_PROCESS_GROUP.
        unsafe {
            GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid_u32);
        }
    }

    // Give the process up to 2 seconds to exit.
    let grace_period = Duration::from_secs(2);
    let poll_interval = Duration::from_millis(100);

    let start = std::time::Instant::now();
    while start.elapsed() < grace_period {
        if !is_alive(pid_u32) {
            still_running = false;
            break;
        }
        tokio::time::sleep(poll_interval).await;
    }

    //---------------------------------------------------------------------
    // Step 2 -- force kill if necessary.
    //---------------------------------------------------------------------

    if still_running {
        #[cfg(unix)]
        {
            let pgid = -(pid_u32 as i32);
            unsafe {
                libc::kill(pgid, libc::SIGKILL);
            }
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::Foundation::HANDLE;
            use windows_sys::Win32::System::Threading::OpenProcess;
            use windows_sys::Win32::System::Threading::TerminateProcess;
            use windows_sys::Win32::System::Threading::PROCESS_TERMINATE;

            unsafe {
                let handle: HANDLE = OpenProcess(PROCESS_TERMINATE, 0, pid_u32);
                if handle != 0 {
                    TerminateProcess(handle, 1);
                    CloseHandle(handle);
                }
            }
        }
    }

    Ok(())
}

/// Remove the session directory and all its contents.
pub fn purge(id: &str) -> Result<()> {
    let paths = paths_for(id)?;
    if paths.dir.exists() {
        std::fs::remove_dir_all(paths.dir)?;
    }
    Ok(())
}
