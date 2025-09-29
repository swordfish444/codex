use std::sync::Arc;

use tokio::sync::Mutex;

use crate::services::McpInterface;
use crate::services::Notifier;
use crate::services::RolloutSink;
use crate::services::SandboxManager;

/// Aggregated services that back a running agent session. Hosts provide
/// implementations for these traits and hand them to the runtime at spawn.
pub struct SessionServices {
    pub mcp: Arc<dyn McpInterface>,
    pub notifier: Arc<dyn Notifier>,
    pub sandbox: Arc<dyn SandboxManager>,
    pub rollout: Mutex<Option<Arc<dyn RolloutSink>>>,
    pub show_raw_agent_reasoning: bool,
}
