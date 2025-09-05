use std::time::Duration;

// Environment filter data models for the TUI
#[derive(Clone, Debug, Default)]
pub struct EnvironmentRow {
    pub id: String,
    pub label: Option<String>,
    pub is_pinned: bool,
    pub repo_hints: Option<String>, // e.g., "openai/codex"
}

#[derive(Clone, Debug, Default)]
pub struct EnvModalState {
    pub query: String,
    pub selected: usize,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum ApplyResultLevel {
    Success,
    Partial,
    Error,
}

#[derive(Clone, Debug)]
pub struct ApplyModalState {
    pub task_id: TaskId,
    pub title: String,
    pub result_message: Option<String>,
    pub result_level: Option<ApplyResultLevel>,
    pub skipped_paths: Vec<String>,
    pub conflict_paths: Vec<String>,
}

use crate::scrollable_diff::ScrollableDiff;
use codex_cloud_tasks_api::CloudBackend;
use codex_cloud_tasks_api::DiffSummary;
use codex_cloud_tasks_api::TaskId;
use codex_cloud_tasks_api::TaskSummary;
use throbber_widgets_tui::ThrobberState;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Default)]
pub struct App {
    pub tasks: Vec<TaskSummary>,
    pub selected: usize,
    pub status: String,
    pub diff_overlay: Option<DiffOverlay>,
    pub throbber: ThrobberState,
    pub refresh_inflight: bool,
    pub details_inflight: bool,
    // Environment filter state
    pub env_filter: Option<String>,
    pub env_modal: Option<EnvModalState>,
    pub apply_modal: Option<ApplyModalState>,
    pub environments: Vec<EnvironmentRow>,
    pub env_last_loaded: Option<std::time::Instant>,
    pub env_loading: bool,
    pub env_error: Option<String>,
    // New Task page
    pub new_task: Option<crate::new_task::NewTaskPage>,
    // Apply preflight spinner state
    pub apply_preflight_inflight: bool,
    // Background enrichment coordination
    pub list_generation: u64,
    pub in_flight: std::collections::HashSet<String>,
    pub summary_cache: std::collections::HashMap<String, (DiffSummary, std::time::Instant)>,
    pub no_diff_yet: std::collections::HashSet<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            selected: 0,
            status: "Press r to refresh".to_string(),
            diff_overlay: None,
            throbber: ThrobberState::default(),
            refresh_inflight: false,
            details_inflight: false,
            env_filter: None,
            env_modal: None,
            apply_modal: None,
            environments: Vec::new(),
            env_last_loaded: None,
            env_loading: false,
            env_error: None,
            new_task: None,
            apply_preflight_inflight: false,
            list_generation: 0,
            in_flight: std::collections::HashSet::new(),
            summary_cache: std::collections::HashMap::new(),
            no_diff_yet: std::collections::HashSet::new(),
        }
    }

    pub fn next(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.tasks.len().saturating_sub(1));
    }

    pub fn prev(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
}

pub async fn load_tasks(
    backend: &dyn CloudBackend,
    env: Option<&str>,
) -> anyhow::Result<Vec<TaskSummary>> {
    // In later milestones, add a small debounce, spinner, and error display.
    let tasks = tokio::time::timeout(Duration::from_secs(5), backend.list_tasks(env)).await??;
    Ok(tasks)
}

pub struct DiffOverlay {
    pub title: String,
    pub task_id: TaskId,
    pub sd: ScrollableDiff,
    pub can_apply: bool,
}

/// Internal app events delivered from background tasks.
/// These let the UI event loop remain responsive and keep the spinner animating.
#[derive(Debug)]
pub enum AppEvent {
    TasksLoaded {
        env: Option<String>,
        result: anyhow::Result<Vec<TaskSummary>>,
    },
    /// Background diff summary computed for a task (or determined absent)
    TaskSummaryUpdated {
        generation: u64,
        id: TaskId,
        summary: DiffSummary,
        no_diff_yet: bool,
        environment_id: Option<String>,
    },
    /// Autodetection of a likely environment id finished
    EnvironmentAutodetected(anyhow::Result<crate::env_detect::AutodetectSelection>),
    /// Background completion of environment list fetch
    EnvironmentsLoaded(anyhow::Result<Vec<EnvironmentRow>>),
    DetailsDiffLoaded {
        id: TaskId,
        title: String,
        diff: String,
    },
    DetailsMessagesLoaded {
        id: TaskId,
        title: String,
        messages: Vec<String>,
    },
    DetailsFailed {
        id: TaskId,
        title: String,
        error: String,
    },
    /// Background completion of new task submission
    NewTaskSubmitted(Result<codex_cloud_tasks_api::CreatedTask, String>),
    /// Background completion of apply preflight when opening modal or on demand
    ApplyPreflightFinished {
        id: TaskId,
        title: String,
        message: String,
        level: ApplyResultLevel,
        skipped: Vec<String>,
        conflicts: Vec<String>,
    },
}

pub type AppEventTx = UnboundedSender<AppEvent>;
pub type AppEventRx = UnboundedReceiver<AppEvent>;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    struct FakeBackend {
        // maps env key to titles
        by_env: std::collections::HashMap<Option<String>, Vec<&'static str>>,
    }

    #[async_trait::async_trait]
    impl codex_cloud_tasks_api::CloudBackend for FakeBackend {
        async fn list_tasks(
            &self,
            env: Option<&str>,
        ) -> codex_cloud_tasks_api::Result<Vec<TaskSummary>> {
            let key = env.map(|s| s.to_string());
            let titles = self
                .by_env
                .get(&key)
                .cloned()
                .unwrap_or_else(|| vec!["default-a", "default-b"]);
            let mut out = Vec::new();
            for (i, t) in titles.into_iter().enumerate() {
                out.push(TaskSummary {
                    id: TaskId(format!("T-{i}")),
                    title: t.to_string(),
                    status: codex_cloud_tasks_api::TaskStatus::Ready,
                    updated_at: Utc::now(),
                    environment_id: env.map(|s| s.to_string()),
                    environment_label: None,
                    summary: codex_cloud_tasks_api::DiffSummary::default(),
                });
            }
            Ok(out)
        }

        async fn get_task_diff(&self, _id: TaskId) -> codex_cloud_tasks_api::Result<String> {
            Err(codex_cloud_tasks_api::Error::Unimplemented(
                "not used in test",
            ))
        }

        async fn get_task_messages(
            &self,
            _id: TaskId,
        ) -> codex_cloud_tasks_api::Result<Vec<String>> {
            Ok(vec![])
        }

        async fn apply_task(
            &self,
            _id: TaskId,
        ) -> codex_cloud_tasks_api::Result<codex_cloud_tasks_api::ApplyOutcome> {
            Err(codex_cloud_tasks_api::Error::Unimplemented(
                "not used in test",
            ))
        }

        async fn create_task(
            &self,
            _env_id: &str,
            _prompt: &str,
            _git_ref: &str,
            _qa_mode: bool,
        ) -> codex_cloud_tasks_api::Result<codex_cloud_tasks_api::CreatedTask> {
            Err(codex_cloud_tasks_api::Error::Unimplemented(
                "not used in test",
            ))
        }
    }

    #[tokio::test]
    async fn load_tasks_uses_env_parameter() {
        // Arrange: env-specific task titles
        let mut by_env = std::collections::HashMap::new();
        by_env.insert(None, vec!["root-1", "root-2"]);
        by_env.insert(Some("env-A".to_string()), vec!["A-1"]);
        by_env.insert(Some("env-B".to_string()), vec!["B-1", "B-2", "B-3"]);
        let backend = FakeBackend { by_env };

        // Act + Assert
        let root = load_tasks(&backend, None).await.unwrap();
        assert_eq!(root.len(), 2);
        assert_eq!(root[0].title, "root-1");

        let a = load_tasks(&backend, Some("env-A")).await.unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].title, "A-1");

        let b = load_tasks(&backend, Some("env-B")).await.unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(b[2].title, "B-3");
    }
}
