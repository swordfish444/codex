//! Session bookkeeping – on-disk layout and simple helpers.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub dir: PathBuf,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    pub meta: PathBuf,
}

/// Calculate canonical paths for the given session ID.
pub fn paths_for(id: &str) -> Result<Paths> {
    let dir = base_dir()?.join(id);
    Ok(Paths {
        dir: dir.clone(),
        stdout: dir.join("stdout.log"),
        stderr: dir.join("stderr.log"),
        meta: dir.join("meta.json"),
    })
}

fn base_dir() -> Result<PathBuf> {
    // ~/.codex/sessions
    let home = dirs::home_dir().context("could not resolve home directory")?;
    Ok(home.join(".codex").join("sessions"))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SessionMeta {
    pub id: String,
    pub pid: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,
}

/// Create the on-disk directory structure and write metadata + empty log files.
/// Create directory & empty log files. Does **not** write metadata; caller should write that
/// once the child process has actually been spawned so we can record its PID.
pub fn prepare_dirs(paths: &Paths) -> Result<()> {
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
    let list = list_sessions_sorted()?;

    // numeric index
    if let Ok(idx) = sel.parse::<usize>() {
        return list.get(idx)
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
pub async fn kill_session(_id: &str) -> Result<()> {
    // TODO: record PID at spawn time and terminate here.
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
