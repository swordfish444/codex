use std::collections::HashMap;
use std::path::PathBuf;

use crate::config_types::History;
use crate::config_types::McpServerConfig;
use crate::config_types::ShellEnvironmentPolicy;
use crate::model_family::ModelFamily;
use crate::model_provider::ModelProviderInfo;
use codex_protocol::config_types::ReasoningEffort;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::config_types::Verbosity;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::SandboxPolicy;

/// Configuration surface consumed by the agent runtime regardless of host.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentConfig {
    pub model: String,
    pub review_model: String,
    pub model_family: ModelFamily,
    pub model_context_window: Option<u64>,
    pub model_auto_compact_token_limit: Option<i64>,
    pub model_reasoning_effort: Option<ReasoningEffort>,
    pub model_reasoning_summary: ReasoningSummary,
    pub model_verbosity: Option<Verbosity>,
    pub model_provider: ModelProviderInfo,
    pub approval_policy: AskForApproval,
    pub sandbox_policy: SandboxPolicy,
    pub shell_environment_policy: ShellEnvironmentPolicy,
    pub user_instructions: Option<String>,
    pub base_instructions: Option<String>,
    pub notify: Option<Vec<String>>,
    pub cwd: PathBuf,
    pub codex_home: PathBuf,
    pub history: History,
    pub mcp_servers: HashMap<String, McpServerConfig>,
    pub include_plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub include_view_image_tool: bool,
    pub tools_web_search_request: bool,
    pub use_experimental_streamable_shell_tool: bool,
    pub use_experimental_unified_exec_tool: bool,
    pub show_raw_agent_reasoning: bool,
    pub codex_linux_sandbox_exe: Option<PathBuf>,
    pub project_doc_max_bytes: usize,
}
