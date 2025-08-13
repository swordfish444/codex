use std::path::PathBuf;
use std::sync::Arc;

use codex_core::ConversationManager;
use codex_core::NewConversation;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use mcp_types::JSONRPCErrorError;
use mcp_types::RequestId;

use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::json_to_toml::json_to_toml;
use crate::outgoing_message::OutgoingMessageSender;
use crate::wire_format::CodexRequest;
use crate::wire_format::ConversationId;
use crate::wire_format::NewConversationParams;
use crate::wire_format::NewConversationResponse;

/// Handles JSON-RPC messages for Codex conversations.
pub(crate) struct CodexMessageProcessor {
    conversation_manager: Arc<ConversationManager>,
    outgoing: Arc<OutgoingMessageSender>,
    codex_linux_sandbox_exe: Option<PathBuf>,
}

impl CodexMessageProcessor {
    pub fn new(
        conversation_manager: Arc<ConversationManager>,
        outgoing: Arc<OutgoingMessageSender>,
        codex_linux_sandbox_exe: Option<PathBuf>,
    ) -> Self {
        Self {
            conversation_manager,
            outgoing,
            codex_linux_sandbox_exe,
        }
    }

    pub async fn process_request(&self, request: CodexRequest) {
        match request {
            CodexRequest::NewConversation {
                id: request_id,
                params,
            } => {
                // Do not tokio::spawn() to process new_conversation()
                // asynchronously because we need to ensure the conversation is
                // created before processing any subsequent messages.
                self.process_new_conversation(request_id, params).await;
            }
        }
    }

    async fn process_new_conversation(&self, request_id: RequestId, params: NewConversationParams) {
        let config = match derive_config(params, self.codex_linux_sandbox_exe.clone()) {
            Ok(config) => config,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("Error deriving config: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        match self.conversation_manager.new_conversation(config).await {
            Ok(conversation_id) => {
                let NewConversation {
                    conversation_id,
                    session_configured,
                    ..
                } = conversation_id;
                let response = NewConversationResponse {
                    conversation_id: ConversationId(conversation_id),
                    model: session_configured.model,
                };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("Error creating conversation: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }
}

fn derive_config(
    params: NewConversationParams,
    codex_linux_sandbox_exe: Option<PathBuf>,
) -> std::io::Result<Config> {
    let NewConversationParams {
        model,
        profile,
        cwd,
        approval_policy,
        sandbox,
        config: cli_overrides,
        base_instructions,
        include_plan_tool,
    } = params;
    let overrides = ConfigOverrides {
        model,
        config_profile: profile,
        cwd: cwd.map(PathBuf::from),
        approval_policy: approval_policy.map(Into::into),
        sandbox_mode: sandbox.map(Into::into),
        model_provider: None,
        codex_linux_sandbox_exe,
        base_instructions,
        include_plan_tool,
        disable_response_storage: None,
        show_raw_agent_reasoning: None,
    };

    let cli_overrides = cli_overrides
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, json_to_toml(v)))
        .collect();

    Config::load_with_cli_overrides(cli_overrides, overrides)
}
