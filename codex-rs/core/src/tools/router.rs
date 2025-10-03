use std::collections::HashMap;

use crate::client_common::tools::ToolSpec;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolCapabilities;
use crate::tools::registry::ToolRegistry;
use crate::tools::spec::ToolsConfig;
use crate::tools::spec::build_specs;
use crate::turn_diff_tracker::TurnDiffTracker;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::ShellToolCallParams;

#[derive(Clone)]
pub struct ToolCall {
    pub tool_name: String,
    pub call_id: String,
    pub payload: ToolPayload,
    pub capabilities: ToolCapabilities,
}

#[derive(Clone)]
pub struct ToolRouter {
    registry: ToolRegistry,
    specs: Vec<ToolSpec>,
}

impl ToolRouter {
    pub fn from_config(
        config: &ToolsConfig,
        mcp_tools: Option<HashMap<String, mcp_types::Tool>>,
    ) -> Self {
        let builder = build_specs(config, mcp_tools);
        let (specs, registry) = builder.build();
        Self { registry, specs }
    }

    pub fn specs(&self) -> &[ToolSpec] {
        &self.specs
    }

    pub fn has_read_only_tools(&self) -> bool {
        self.registry.has_read_only_tools()
    }

    pub fn build_tool_call(
        &self,
        session: &Session,
        item: ResponseItem,
    ) -> Result<Option<ToolCall>, FunctionCallError> {
        match item {
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                if let Some((server, tool)) = session.parse_mcp_tool_name(&name) {
                    Ok(Some(self.attach_capabilities(ToolCall {
                        tool_name: name,
                        call_id,
                        payload: ToolPayload::Mcp {
                            server,
                            tool,
                            raw_arguments: arguments,
                        },
                        capabilities: ToolCapabilities::mutating(),
                    })))
                } else {
                    let payload = if name == "unified_exec" {
                        ToolPayload::UnifiedExec { arguments }
                    } else {
                        ToolPayload::Function { arguments }
                    };
                    Ok(Some(self.attach_capabilities(ToolCall {
                        tool_name: name,
                        call_id,
                        payload,
                        capabilities: ToolCapabilities::mutating(),
                    })))
                }
            }
            ResponseItem::CustomToolCall {
                name,
                input,
                call_id,
                ..
            } => Ok(Some(self.attach_capabilities(ToolCall {
                tool_name: name,
                call_id,
                payload: ToolPayload::Custom { input },
                capabilities: ToolCapabilities::mutating(),
            }))),
            ResponseItem::LocalShellCall {
                id,
                call_id,
                action,
                ..
            } => {
                let call_id = call_id
                    .or(id)
                    .ok_or(FunctionCallError::MissingLocalShellCallId)?;

                match action {
                    LocalShellAction::Exec(exec) => {
                        let params = ShellToolCallParams {
                            command: exec.command,
                            workdir: exec.working_directory,
                            timeout_ms: exec.timeout_ms,
                            with_escalated_permissions: None,
                            justification: None,
                        };
                        Ok(Some(self.attach_capabilities(ToolCall {
                            tool_name: "local_shell".to_string(),
                            call_id,
                            payload: ToolPayload::LocalShell { params },
                            capabilities: ToolCapabilities::mutating(),
                        })))
                    }
                }
            }
            _ => Ok(None),
        }
    }

    fn attach_capabilities(&self, mut call: ToolCall) -> ToolCall {
        if let Some(capabilities) = self.registry.capabilities(call.tool_name.as_str()) {
            call.capabilities = capabilities;
        }
        call
    }

    pub async fn dispatch_tool_call(
        &self,
        session: &Session,
        turn: &TurnContext,
        tracker: &mut TurnDiffTracker,
        sub_id: &str,
        call: ToolCall,
    ) -> Result<ResponseInputItem, FunctionCallError> {
        let ToolCall {
            tool_name,
            call_id,
            payload,
            ..
        } = call;
        let payload_outputs_custom = matches!(payload, ToolPayload::Custom { .. });
        let failure_call_id = call_id.clone();

        let invocation = ToolInvocation {
            session,
            turn,
            tracker,
            sub_id,
            call_id,
            tool_name,
            payload,
        };

        match self.registry.dispatch(invocation).await {
            Ok(response) => Ok(response),
            Err(FunctionCallError::Fatal(message)) => Err(FunctionCallError::Fatal(message)),
            Err(err) => Ok(Self::failure_response(
                failure_call_id,
                payload_outputs_custom,
                err,
            )),
        }
    }

    fn failure_response(
        call_id: String,
        payload_outputs_custom: bool,
        err: FunctionCallError,
    ) -> ResponseInputItem {
        let message = err.to_string();
        if payload_outputs_custom {
            ResponseInputItem::CustomToolCallOutput {
                call_id,
                output: message,
            }
        } else {
            ResponseInputItem::FunctionCallOutput {
                call_id,
                output: codex_protocol::models::FunctionCallOutputPayload {
                    content: message,
                    success: Some(false),
                },
            }
        }
    }
}
