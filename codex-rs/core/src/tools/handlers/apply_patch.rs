use std::collections::HashMap;

use crate::apply_patch;
use crate::apply_patch::ApplyPatchExec;
use crate::apply_patch::InternalApplyPatchInvocation;
use crate::apply_patch::convert_apply_patch_to_protocol;
use crate::codex::ApplyPatchCommandContext;
use crate::codex::ExecCommandContext;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::error::CodexErr;
use crate::error::SandboxErr;
use crate::exec::ExecParams;
use crate::exec::ExecToolCallOutput;
use crate::exec::StdoutStream;
use crate::executor::ExecutionMode;
use crate::executor::errors::ExecError;
use crate::executor::linkers::PreparedExec;
use crate::function_tool::FunctionCallError;
use crate::tools::{handle_container_exec_with_params, MODEL_FORMAT_HEAD_BYTES};
use crate::tools::MODEL_FORMAT_HEAD_LINES;
use crate::tools::MODEL_FORMAT_MAX_BYTES;
use crate::tools::MODEL_FORMAT_MAX_LINES;
use crate::tools::MODEL_FORMAT_TAIL_LINES;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::spec::ApplyPatchToolArgs;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use codex_apply_patch::MaybeApplyPatchVerified;
use codex_apply_patch::maybe_parse_apply_patch_verified;
use codex_protocol::protocol::AskForApproval;
use codex_utils_string::take_bytes_at_char_boundary;
use codex_utils_string::take_last_bytes_at_char_boundary;
use serde::Serialize;
use tracing::trace;

pub struct ApplyPatchHandler;

#[async_trait]
impl ToolHandler for ApplyPatchHandler {
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            payload,
            ToolPayload::Function { .. } | ToolPayload::Custom { .. }
        )
    }

    async fn handle(
        &self,
        invocation: ToolInvocation<'_>,
    ) -> Result<ToolOutput, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            tracker,
            sub_id,
            call_id,
            tool_name,
            payload,
        } = invocation;

        let patch_input = match payload {
            ToolPayload::Function { arguments } => {
                let args: ApplyPatchToolArgs = serde_json::from_str(&arguments).map_err(|e| {
                    FunctionCallError::RespondToModel(format!(
                        "failed to parse function arguments: {e:?}"
                    ))
                })?;
                args.input
            }
            ToolPayload::Custom { input } => input,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "apply_patch handler received unsupported payload".to_string(),
                ));
            }
        };

        let exec_params = ExecParams {
            command: vec!["apply_patch".to_string(), patch_input.clone()],
            cwd: turn.cwd.clone(),
            timeout_ms: None,
            env: HashMap::new(),
            with_escalated_permissions: None,
            justification: None,
        };

        let content = handle_container_exec_with_params(
            tool_name.as_str(),
            exec_params,
            session,
            turn,
            tracker,
            sub_id.to_string(),
            call_id.clone(),
        )
        .await?;

        Ok(ToolOutput::Function {
            content,
            success: true,
        })
    }
}
