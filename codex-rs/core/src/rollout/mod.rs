use std::path::Path;

use async_trait::async_trait;
use codex_agent::rollout::GitInfoCollector as SharedGitInfoCollector;
use codex_protocol::protocol::GitInfo;

pub use codex_agent::rollout::ARCHIVED_SESSIONS_SUBDIR;
pub use codex_agent::rollout::RolloutConfig;
pub use codex_agent::rollout::RolloutRecorder;
pub use codex_agent::rollout::RolloutRecorderParams;
pub use codex_agent::rollout::SESSIONS_SUBDIR;

pub mod list {
    pub use codex_agent::rollout::list::*;
}

#[cfg(test)]
pub mod tests;

use crate::git_info::collect_git_info;

pub struct CoreGitInfoCollector;

#[async_trait]
impl SharedGitInfoCollector for CoreGitInfoCollector {
    async fn collect(&self, cwd: &Path) -> Option<GitInfo> {
        collect_git_info(cwd).await
    }
}
