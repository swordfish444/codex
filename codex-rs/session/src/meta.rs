//! Rich on-disk session metadata envelope.
//!
//! The file is written as `meta.json` inside every session directory so users
//! (and other tools) can inspect how a particular session was started even
//! months later.  Keeping the full CLI invocation together with a few extra
//! bits of contextual information (like the git commit of the build) makes
//! debugging and reproducibility significantly easier.

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;

use crate::store::SessionKind;

/// The CLI configuration that was used to launch the underlying agent.
///
/// Depending on the chosen agent flavour (`codex-exec` vs `codex-repl`) the
/// contained configuration differs.  We use an *externally tagged* enum so
/// the JSON clearly states which variant was used while still keeping the
/// nested structure as-is.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "agent", rename_all = "lowercase")]
pub enum AgentCli {
    /// Non-interactive batch agent.
    Exec(codex_exec::Cli),

    /// Interactive REPL agent (only available on Unix-like systems).
    #[cfg(unix)]
    Repl(codex_repl::Cli),
}

/// Versioned envelope that is persisted to disk.
///
/// A monotonically increasing `version` field allows us to evolve the schema
/// over time while still being able to parse *older* files.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Unique identifier – also doubles as the directory name.
    pub id: String,

    /// Process ID of the *leader* process belonging to the session.
    pub pid: u32,

    /// Whether the session is an `exec` or `repl` one.
    pub kind: SessionKind,

    /// Complete CLI configuration that was used to spawn the agent.
    pub cli: AgentCli,

    /// Short preview of the natural-language prompt (if present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,

    /// Wall-clock timestamp when the session was created.
    pub created_at: DateTime<Utc>,

    /// Git commit hash of the `codex-rs` build that produced this file.
    pub codex_commit: String,

    /// Schema version so we can migrate later.
    pub version: u8,
}

impl SessionMeta {
    /// Bump this whenever the structure changes in a backwards-incompatible
    /// way.
    pub const CURRENT_VERSION: u8 = 1;

    /// Convenience constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        pid: u32,
        kind: SessionKind,
        cli: AgentCli,
        prompt_preview: Option<String>,
    ) -> Self {
        Self {
            id,
            pid,
            kind,
            cli,
            prompt_preview,
            created_at: Utc::now(),
            codex_commit: crate::build::git_sha().to_owned(),
            version: Self::CURRENT_VERSION,
        }
    }
}
