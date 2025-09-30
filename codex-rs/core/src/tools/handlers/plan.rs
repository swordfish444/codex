use async_trait::async_trait;

use crate::function_tool::FunctionCallError;
use crate::plan_tool::handle_update_plan;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct PlanHandler;

#[async_trait]
impl ToolHandler for PlanHandler {
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(
        &self,
        invocation: ToolInvocation<'_>,
    ) -> Result<ToolOutput, FunctionCallError> {
        let ToolInvocation {
            session,
            sub_id,
            call_id,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "update_plan handler received unsupported payload".to_string(),
                ));
            }
        };

        let content = handle_update_plan(session, arguments, sub_id.to_string(), call_id).await?;

        Ok(ToolOutput::Function {
            content,
            success: true,
        })
    }
}
