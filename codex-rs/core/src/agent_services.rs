use anyhow::Result;
use async_trait::async_trait;
use mcp_types::CallToolResult;
use mcp_types::Tool;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::auth::AuthManager;
use crate::auth::CodexAuth;
use crate::exec_command::ExecCommandOutput;
use crate::exec_command::ExecCommandParams;
use crate::exec_command::ExecSessionManager;
use crate::exec_command::WriteStdinParams;
use crate::mcp_connection_manager::McpConnectionManager;
use crate::rollout::RolloutRecorder;
use crate::unified_exec::UnifiedExecError;
use crate::unified_exec::UnifiedExecRequest;
use crate::unified_exec::UnifiedExecResult;
use crate::unified_exec::UnifiedExecSessionManager;
use crate::user_notification::UserNotification;
use crate::user_notification::UserNotifier;

/// Provides access to credentials required when talking to model providers.
#[async_trait]
pub trait CredentialsProvider: Send + Sync {
    fn auth(&self) -> Option<CodexAuth>;

    async fn refresh_token(&self) -> std::io::Result<Option<String>>;
}

#[async_trait]
impl CredentialsProvider for AuthManager {
    fn auth(&self) -> Option<CodexAuth> {
        AuthManager::auth(self)
    }

    async fn refresh_token(&self) -> std::io::Result<Option<String>> {
        AuthManager::refresh_token(self).await
    }
}

/// Emits user-facing notifications for turn completion or other events.
pub trait Notifier: Send + Sync {
    fn notify(&self, notification: &UserNotification);
}

impl Notifier for UserNotifier {
    fn notify(&self, notification: &UserNotification) {
        UserNotifier::notify(self, notification);
    }
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
    ) -> Result<CallToolResult>;
}

#[async_trait]
impl McpInterface for McpConnectionManager {
    fn list_all_tools(&self) -> HashMap<String, Tool> {
        McpConnectionManager::list_all_tools(self)
    }

    fn parse_tool_name(&self, tool_name: &str) -> Option<(String, String)> {
        McpConnectionManager::parse_tool_name(self, tool_name)
    }

    async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        McpConnectionManager::call_tool(self, server, tool, arguments).await
    }
}

/// Persists rollout events for later inspection or replay.
#[async_trait]
pub trait RolloutSink: Send + Sync {
    async fn record_items(
        &self,
        items: &[codex_protocol::protocol::RolloutItem],
    ) -> std::io::Result<()>;

    async fn flush(&self) -> std::io::Result<()>;

    async fn shutdown(&self) -> std::io::Result<()>;

    fn get_rollout_path(&self) -> PathBuf;
}

#[async_trait]
impl RolloutSink for RolloutRecorder {
    async fn record_items(
        &self,
        items: &[codex_protocol::protocol::RolloutItem],
    ) -> std::io::Result<()> {
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

/// Default [`SandboxManager`] used by the CLI runtime. Wraps the existing exec
/// session managers and exposes their functionality via the trait-based
/// interface so other hosts can substitute different implementations.
pub struct DefaultSandboxManager {
    exec_session_manager: ExecSessionManager,
    unified_exec_manager: UnifiedExecSessionManager,
    codex_linux_sandbox_exe: Option<PathBuf>,
    user_shell: crate::shell::Shell,
}

impl DefaultSandboxManager {
    pub fn new(
        exec_session_manager: ExecSessionManager,
        unified_exec_manager: UnifiedExecSessionManager,
        codex_linux_sandbox_exe: Option<PathBuf>,
        user_shell: crate::shell::Shell,
    ) -> Self {
        Self {
            exec_session_manager,
            unified_exec_manager,
            codex_linux_sandbox_exe,
            user_shell,
        }
    }
}

#[async_trait]
impl SandboxManager for DefaultSandboxManager {
    async fn handle_exec_command_request(
        &self,
        params: ExecCommandParams,
    ) -> Result<ExecCommandOutput, String> {
        self.exec_session_manager
            .handle_exec_command_request(params)
            .await
    }

    async fn handle_write_stdin_request(
        &self,
        params: WriteStdinParams,
    ) -> Result<ExecCommandOutput, String> {
        self.exec_session_manager
            .handle_write_stdin_request(params)
            .await
    }

    async fn handle_unified_exec_request(
        &self,
        request: UnifiedExecRequest<'_>,
    ) -> Result<UnifiedExecResult, UnifiedExecError> {
        self.unified_exec_manager.handle_request(request).await
    }

    fn codex_linux_sandbox_exe(&self) -> &Option<PathBuf> {
        &self.codex_linux_sandbox_exe
    }

    fn user_shell(&self) -> &crate::shell::Shell {
        &self.user_shell
    }
}
