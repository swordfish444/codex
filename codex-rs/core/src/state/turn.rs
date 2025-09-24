use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use codex_utils_readiness::ReadinessFlag;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::client::ModelClient;
use crate::config_types::ShellEnvironmentPolicy;
use crate::openai_tools::ToolsConfig;
use crate::protocol::AskForApproval;
use crate::protocol::InputItem;
use crate::protocol::SandboxPolicy;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;

#[derive(Debug)]
pub(crate) struct TurnContext {
    pub(crate) client: ModelClient,
    /// The session's current working directory. All relative paths provided by
    /// the model as well as sandbox policies are resolved against this path
    /// instead of `std::env::current_dir()`.
    pub(crate) cwd: PathBuf,
    pub(crate) base_instructions: Option<String>,
    pub(crate) user_instructions: Option<String>,
    pub(crate) approval_policy: AskForApproval,
    pub(crate) sandbox_policy: SandboxPolicy,
    pub(crate) shell_environment_policy: ShellEnvironmentPolicy,
    pub(crate) tools_config: ToolsConfig,
    pub(crate) is_review_mode: bool,
    pub(crate) final_output_json_schema: Option<Value>,
}

impl TurnContext {
    pub(crate) fn resolve_path(&self, path: Option<String>) -> PathBuf {
        path.as_ref()
            .map(PathBuf::from)
            .map_or_else(|| self.cwd.clone(), |p| self.cwd.join(p))
    }
}

#[derive(Default)]
struct TurnMailbox {
    latest_readiness: Option<Arc<ReadinessFlag>>,
    pending: VecDeque<ResponseInputItem>,
}

pub(crate) struct TurnState {
    sub_id: String,
    turn_context: Arc<TurnContext>,
    initial_input: Option<ResponseInputItem>,
    initial_readiness: Option<Arc<ReadinessFlag>>,
    mailbox: Mutex<TurnMailbox>,
}

impl TurnState {
    pub(crate) fn new(
        sub_id: String,
        turn_context: Arc<TurnContext>,
        initial_input: Vec<InputItem>,
        readiness: Option<Arc<ReadinessFlag>>,
    ) -> Self {
        let initial_input = if initial_input.is_empty() {
            None
        } else {
            Some(initial_input.into())
        };

        Self {
            sub_id,
            turn_context,
            initial_input,
            initial_readiness: readiness,
            mailbox: Mutex::new(TurnMailbox::default()),
        }
    }

    pub(crate) fn sub_id(&self) -> &str {
        &self.sub_id
    }

    pub(crate) fn turn_context(&self) -> Arc<TurnContext> {
        Arc::clone(&self.turn_context)
    }

    pub(crate) fn initial_input(&self) -> Option<ResponseInputItem> {
        self.initial_input.clone()
    }

    pub(crate) fn initial_readiness(&self) -> Option<Arc<ReadinessFlag>> {
        self.initial_readiness.clone()
    }

    pub(crate) async fn enqueue_user_input(
        &self,
        items: Vec<InputItem>,
        readiness: Option<Arc<ReadinessFlag>>,
    ) {
        let mut mailbox = self.mailbox.lock().await;
        if let Some(flag) = readiness {
            mailbox.latest_readiness = Some(flag);
        }
        if items.is_empty() {
            return;
        }
        let input: ResponseInputItem = items.into();
        mailbox.pending.push_back(input);
    }

    pub(crate) async fn drain_mailbox(
        &self,
        current: Option<Arc<ReadinessFlag>>,
    ) -> (Vec<ResponseItem>, Option<Arc<ReadinessFlag>>) {
        let mut mailbox = self.mailbox.lock().await;
        let readiness = mailbox.latest_readiness.take().or(current);
        let items = mailbox.pending.drain(..).map(ResponseItem::from).collect();
        (items, readiness)
    }
}
