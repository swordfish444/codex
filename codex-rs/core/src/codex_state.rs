//! Persistence layer for the global, mutable project state store
//!
//! The project state is stored at `~/.codex/project_state.json` as a JSON
//! object. The object has the following schema:
//!
//! ```json
//! {
//!     "projects": {
//!         "</abs/path/to/project>": {
//!             "trusted": <boolean>,
//!             // more to come...
//!         }
//!     },
//! }
//! ```
//!
//! To avoid race conditions, we leverage advisory file locking to ensure that
//! only one process can read or write the file at a time.

use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Result;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::config::Config;
use crate::util::acquire_exclusive_lock_with_retry;
#[cfg(unix)]
use crate::util::acquire_shared_lock_with_retry;
use crate::util::ensure_owner_only_permissions;

/// Filename that stores the project state inside `~/.codex`.
const CODEX_STATE_FILENAME: &str = "codex-state.json";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub trusted: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct CodexState {
    pub projects: HashMap<PathBuf, Project>,
}

fn codex_state_filepath(config: &Config) -> PathBuf {
    let mut path = config.codex_home.clone();
    path.push(CODEX_STATE_FILENAME);
    path
}

async fn open_or_create_codex_state_file(config: &Config) -> Result<File> {
    // Resolve `~/.codex/codex-state.json` and ensure the parent directory exists.
    let path = codex_state_filepath(config);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let file: File = match OpenOptions::new()
        .read(true)
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "failed to open project state file");
            return Err(e);
        }
    };

    // Ensure restrictive permissions (0600) on Unix.
    #[cfg(unix)]
    ensure_owner_only_permissions(&file).await?;

    Ok(file)
}

/// Lookup current project's state, creating it if needed.
#[cfg(unix)]
pub async fn lookup_project(config: &Config) -> Result<Project> {
    use std::io::BufReader;

    let file = open_or_create_codex_state_file(config).await?;

    // Acquire a shared lock for reading.
    if let Err(e) = acquire_shared_lock_with_retry(&file) {
        tracing::warn!(error = %e, "failed to acquire shared lock on project state file");
        return Err(e);
    }

    // Try to parse JSON; if empty or invalid, fall back to default state.
    let mut reader = BufReader::new(&file);
    let mut buf = String::new();
    match reader.read_to_string(&mut buf) {
        Ok(_) => {
            let codex_state: CodexState = if buf.trim().is_empty() {
                CodexState::default()
            } else {
                match serde_json::from_str(&buf) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to parse project state file; using defaults");
                        CodexState::default()
                    }
                }
            };
            let project = codex_state
                .projects
                .get(&config.cwd)
                .cloned()
                .unwrap_or(Project { trusted: false });
            Ok(project)
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to read project state file");
            Err(e)
        }
    }
}

/// Not yet supported on non‑Unix platforms.
#[cfg(not(unix))]
pub async fn lookup_project(_config: &Config) -> Result<Project> {
    return Ok(Project { trusted: false });
}

/// Update the project state for the given project. This function will
/// (currently) read and write the entire file, which does not scale well.
/// Use this function sparingly until we implement a more efficient solution.
pub async fn update_project(config: &Config, project: &Project) -> Result<()> {
    let mut file = open_or_create_codex_state_file(config).await?;

    // Open & lock file for writing.
    if let Err(e) = acquire_exclusive_lock_with_retry(&file).await {
        tracing::warn!(error = %e, "failed to acquire exclusive lock on project state file");
        return Err(e);
    }

    // Ensure file permissions.
    ensure_owner_only_permissions(&file).await?;

    // Read existing state (if any).
    let mut contents = String::new();
    // Safety: reading from start; ensure cursor at 0.
    file.seek(SeekFrom::Start(0))?;
    let _ = file.read_to_string(&mut contents);
    let mut codex_state: CodexState = if contents.trim().is_empty() {
        CodexState::default()
    } else {
        serde_json::from_str(&contents).unwrap_or_default()
    };
    let cwd = config.cwd.clone();
    codex_state.projects.insert(cwd, project.clone());

    // Overwrite the file from the beginning
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    serde_json::to_writer_pretty(&mut file, &codex_state)?;
    file.flush()?;

    Ok(())
}

/// Not yet supported on non‑Unix platforms.
#[cfg(not(unix))]
pub async fn update_project(_config: &Config, _project: &Project) -> Result<()> {
    return Ok(());
}
