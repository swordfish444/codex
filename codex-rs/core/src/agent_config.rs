pub use codex_agent::AgentConfig;

use crate::config::Config;

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
