use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::Config;
use crate::config_types::History;
use crate::config_types::McpServerConfig;
use crate::config_types::ShellEnvironmentPolicy;
use crate::model_family::ModelFamily;
use crate::model_provider_info::ModelProviderInfo;
use codex_protocol::config_types::ReasoningEffort;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::config_types::Verbosity;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::SandboxPolicy;

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

impl From<&Config> for AgentConfig {
    fn from(config: &Config) -> Self {
        Self {
            model: config.model.clone(),
            review_model: config.review_model.clone(),
            model_family: config.model_family.clone(),
            model_context_window: config.model_context_window,
            model_auto_compact_token_limit: config.model_auto_compact_token_limit,
            model_reasoning_effort: config.model_reasoning_effort,
            model_reasoning_summary: config.model_reasoning_summary,
            model_verbosity: config.model_verbosity,
            model_provider: config.model_provider.clone(),
            approval_policy: config.approval_policy,
            sandbox_policy: config.sandbox_policy.clone(),
            shell_environment_policy: config.shell_environment_policy.clone(),
            user_instructions: config.user_instructions.clone(),
            base_instructions: config.base_instructions.clone(),
            notify: config.notify.clone(),
            cwd: config.cwd.clone(),
            codex_home: config.codex_home.clone(),
            history: config.history.clone(),
            mcp_servers: config.mcp_servers.clone(),
            include_plan_tool: config.include_plan_tool,
            include_apply_patch_tool: config.include_apply_patch_tool,
            include_view_image_tool: config.include_view_image_tool,
            tools_web_search_request: config.tools_web_search_request,
            use_experimental_streamable_shell_tool: config.use_experimental_streamable_shell_tool,
            use_experimental_unified_exec_tool: config.use_experimental_unified_exec_tool,
            show_raw_agent_reasoning: config.show_raw_agent_reasoning,
            codex_linux_sandbox_exe: config.codex_linux_sandbox_exe.clone(),
            project_doc_max_bytes: config.project_doc_max_bytes,
        }
    }
}
