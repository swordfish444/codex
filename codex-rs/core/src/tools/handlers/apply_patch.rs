use std::collections::HashMap;

use crate::exec::ExecParams;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handle_container_exec_with_params;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::spec::ApplyPatchToolArgs;
use async_trait::async_trait;

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
