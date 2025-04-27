//! Lightweight on-disk session metadata.
//!
//! The metadata is persisted as `meta.json` inside each session directory so
//! users -- or other tooling -- can inspect **how** a session was started even
//! months later.  Instead of serialising the full, typed CLI structs (which
//! would force every agent crate to depend on `serde`) we only keep the raw
//! argument vector that was passed to the spawned process.  This keeps the
//! public API surface minimal while still giving us reproducibility -- a
//! session can always be re-spawned with `codex <args...>`.

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;

use crate::store::SessionKind;

/// JSON envelope version.  Bump when the structure changes in a
/// backwards-incompatible way.
pub const CURRENT_VERSION: u8 = 1;

/// Persisted session metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Unique identifier (also doubles as directory name).
    pub id: String,

    /// Leader process id (PID).
    pub pid: u32,

    /// Whether the session is an `exec` or `repl` one.
    pub kind: SessionKind,

    /// Raw command-line arguments that were used to spawn the agent
    /// (`codex-exec ...` or `codex-repl ...`).
    pub argv: Vec<String>,

    /// Short preview of the user prompt (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,

    /// Wall-clock timestamp when the session was created.
    pub created_at: DateTime<Utc>,

    /// Git commit hash of the build that produced this file.
    pub codex_commit: String,

    /// Schema version (see [`CURRENT_VERSION`]).
    pub version: u8,
}

impl SessionMeta {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        pid: u32,
        kind: SessionKind,
        argv: Vec<String>,
        prompt_preview: Option<String>,
    ) -> Self {
        Self {
            id,
            pid,
            kind,
            argv,
            prompt_preview,
            created_at: Utc::now(),
            codex_commit: crate::build::git_sha().to_owned(),
            version: CURRENT_VERSION,
        }
    }
}
