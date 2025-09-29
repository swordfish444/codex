use crate::agent_services::McpInterface;
use crate::agent_services::Notifier;
use crate::agent_services::RolloutSink;
use crate::agent_services::SandboxManager;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) struct SessionServices {
    pub(crate) mcp: Arc<dyn McpInterface>,
    pub(crate) notifier: Arc<dyn Notifier>,
    pub(crate) sandbox: Arc<dyn SandboxManager>,
    pub(crate) rollout: Mutex<Option<Arc<dyn RolloutSink>>>,
    pub(crate) show_raw_agent_reasoning: bool,
}
