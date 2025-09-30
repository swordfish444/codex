use async_trait::async_trait;
use serde::Deserialize;

use crate::function_tool::FunctionCallError;
use crate::protocol::InputItem;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct ViewImageHandler;

#[derive(Deserialize)]
struct ViewImageArgs {
    path: String,
}

#[async_trait]
impl ToolHandler for ViewImageHandler {
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(
        &self,
        invocation: ToolInvocation<'_>,
    ) -> Result<ToolOutput, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "view_image handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: ViewImageArgs = serde_json::from_str(&arguments).map_err(|e| {
            FunctionCallError::RespondToModel(format!("failed to parse function arguments: {e:?}"))
        })?;

        let abs_path = turn.resolve_path(Some(args.path));
        session
            .inject_input(vec![InputItem::LocalImage { path: abs_path }])
            .await
            .map_err(|_| {
                FunctionCallError::RespondToModel(
                    "unable to attach image (no active task)".to_string(),
                )
            })?;

        Ok(ToolOutput::Function {
            content: "attached local image path".to_string(),
            success: true,
        })
    }
}
