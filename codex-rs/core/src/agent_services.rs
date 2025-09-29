use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use codex_agent::notifications::UserNotification;
use codex_agent::services::CredentialsProvider;
use codex_agent::services::McpInterface;
use codex_agent::services::Notifier;
use codex_agent::services::ProviderAuth;
use codex_agent::services::SandboxManager;
use codex_agent::token_data::PlanType;
use codex_protocol::mcp_protocol::AuthMode;
use mcp_types::CallToolResult;
use mcp_types::Tool;
use serde_json::Value;

use crate::auth::AuthManager;
use crate::auth::CodexAuth;
use crate::exec_command::ExecCommandOutput;
use crate::exec_command::ExecCommandParams;
use crate::exec_command::ExecSessionManager;
use crate::exec_command::WriteStdinParams;
use crate::mcp_connection_manager::McpConnectionManager;
use crate::unified_exec::UnifiedExecError;
use crate::unified_exec::UnifiedExecRequest;
use crate::unified_exec::UnifiedExecResult;
use crate::unified_exec::UnifiedExecSessionManager;
use crate::user_notification::UserNotifier;

#[async_trait]
impl ProviderAuth for CodexAuth {
    fn mode(&self) -> AuthMode {
        self.mode
    }

    async fn access_token(&self) -> std::io::Result<String> {
        self.get_token().await
    }

    fn account_id(&self) -> Option<String> {
        self.get_account_id()
    }

    fn plan_type(&self) -> Option<PlanType> {
        self.get_plan_type()
    }
}

#[async_trait]
impl CredentialsProvider for AuthManager {
    fn auth(&self) -> Option<Arc<dyn ProviderAuth>> {
        AuthManager::auth(self).map(|auth| Arc::new(auth) as Arc<dyn ProviderAuth>)
    }

    async fn refresh_token(&self) -> std::io::Result<Option<String>> {
        AuthManager::refresh_token(self).await
    }
}

impl Notifier for UserNotifier {
    fn notify(&self, notification: &UserNotification) {
        UserNotifier::notify(self, notification);
    }
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
