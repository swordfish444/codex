//! Session bookkeeping – on-disk layout and simple helpers.

use anyhow::{Context, Result};
use directories::ProjectDirs;
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
    let dirs = ProjectDirs::from("dev", "codex", "codex-session")
        .context("unable to resolve data directory")?;
    Ok(dirs.data_dir().to_owned())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SessionMeta {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Create the on-disk directory structure and write metadata + empty log files.
pub fn materialise(paths: &Paths, meta: &SessionMeta) -> Result<()> {
    std::fs::create_dir_all(&paths.dir)?;

    // Metadata (pretty-printed for manual inspection).
    std::fs::write(&paths.meta, serde_json::to_vec_pretty(meta)?)?;

    // Touch stdout/stderr so they exist even before the agent writes.
    for p in [&paths.stdout, &paths.stderr] {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)?;
    }

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
