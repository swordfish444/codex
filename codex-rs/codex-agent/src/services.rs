use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use codex_protocol::mcp_protocol::AuthMode;
use codex_protocol::protocol::RolloutItem;
use mcp_types::Tool;
use serde_json::Value;

use crate::exec_command::ExecCommandOutput;
use crate::exec_command::ExecCommandParams;
use crate::exec_command::WriteStdinParams;
use crate::notifications::UserNotification;
use crate::rollout::RolloutRecorder;
use crate::token_data::PlanType;
use crate::unified_exec::UnifiedExecError;
use crate::unified_exec::UnifiedExecRequest;
use crate::unified_exec::UnifiedExecResult;

/// Authentication context made available to the provider layer.
#[async_trait]
pub trait ProviderAuth: Send + Sync {
    fn mode(&self) -> AuthMode;

    async fn access_token(&self) -> std::io::Result<String>;

    fn account_id(&self) -> Option<String>;

    fn plan_type(&self) -> Option<PlanType>;
}

/// Provides access to credentials required when talking to model providers.
#[async_trait]
pub trait CredentialsProvider: Send + Sync {
    fn auth(&self) -> Option<std::sync::Arc<dyn ProviderAuth>>;

    async fn refresh_token(&self) -> std::io::Result<Option<String>>;
}

/// Emits user-facing notifications for turn completion or other events.
pub trait Notifier: Send + Sync {
    fn notify(&self, notification: &UserNotification);
}

/// Aggregates and dispatches MCP tool calls across configured servers.
#[async_trait]
pub trait McpInterface: Send + Sync {
    fn list_all_tools(&self) -> HashMap<String, Tool>;

    fn parse_tool_name(&self, tool_name: &str) -> Option<(String, String)>;

    async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<Value>,
    ) -> anyhow::Result<mcp_types::CallToolResult>;
}

/// Persists rollout events for later inspection or replay.
#[async_trait]
pub trait RolloutSink: Send + Sync {
    async fn record_items(&self, items: &[RolloutItem]) -> std::io::Result<()>;

    async fn flush(&self) -> std::io::Result<()>;

    async fn shutdown(&self) -> std::io::Result<()>;

    fn get_rollout_path(&self) -> PathBuf;
}

#[async_trait]
impl RolloutSink for RolloutRecorder {
    async fn record_items(&self, items: &[RolloutItem]) -> std::io::Result<()> {
        RolloutRecorder::record_items(self, items).await
    }

    async fn flush(&self) -> std::io::Result<()> {
        RolloutRecorder::flush(self).await
    }

    async fn shutdown(&self) -> std::io::Result<()> {
        RolloutRecorder::shutdown(self).await
    }

    fn get_rollout_path(&self) -> PathBuf {
        RolloutRecorder::get_rollout_path(self)
    }
}

/// Handles sandboxed exec orchestration, including long-running sessions.
#[async_trait]
pub trait SandboxManager: Send + Sync {
    async fn handle_exec_command_request(
        &self,
        params: ExecCommandParams,
    ) -> Result<ExecCommandOutput, String>;

    async fn handle_write_stdin_request(
        &self,
        params: WriteStdinParams,
    ) -> Result<ExecCommandOutput, String>;

    async fn handle_unified_exec_request(
        &self,
        request: UnifiedExecRequest<'_>,
    ) -> Result<UnifiedExecResult, UnifiedExecError>;

    fn codex_linux_sandbox_exe(&self) -> &Option<PathBuf>;

    fn user_shell(&self) -> &crate::shell::Shell;
}
