use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Command-line interface for the non-interactive `codex-exec` agent.
///
/// The struct needs to be serialisable so the full invocation can be stored
/// in the on-disk session `meta.json` for later introspection.
#[derive(Parser, Debug, Clone, Serialize, Deserialize)]
#[command(version)]
pub struct Cli {
    /// Optional image(s) to attach to the initial prompt.
    #[arg(long = "image", short = 'i', value_name = "FILE", value_delimiter = ',', num_args = 1..)]
    pub images: Vec<PathBuf>,

    /// Model the agent should use.
    #[arg(long, short = 'm')]
    pub model: Option<String>,

    /// Allow running Codex outside a Git repository.
    #[arg(long = "skip-git-repo-check", default_value_t = false)]
    pub skip_git_repo_check: bool,

    /// Disable server‑side response storage (sends the full conversation context with every request)
    #[arg(long = "disable-response-storage", default_value_t = false)]
    pub disable_response_storage: bool,

    /// Initial instructions for the agent.
    pub prompt: Option<String>,
}
