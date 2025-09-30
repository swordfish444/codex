use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use codex_protocol::models::ResponseInputItem;
use tracing::warn;

use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Function,
    UnifiedExec,
    Mcp,
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn kind(&self) -> ToolKind;

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            (self.kind(), payload),
            (ToolKind::Function, ToolPayload::Function { .. })
                | (ToolKind::Function, ToolPayload::UnifiedExec { .. })
                | (ToolKind::UnifiedExec, ToolPayload::UnifiedExec { .. })
                | (ToolKind::Mcp, ToolPayload::Mcp { .. })
        )
    }

    async fn handle(&self, invocation: ToolInvocation<'_>)
    -> Result<ToolOutput, FunctionCallError>;
}

pub struct ToolRegistry {
    handlers: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new(handlers: HashMap<String, Arc<dyn ToolHandler>>) -> Self {
        Self { handlers }
    }

    pub fn handler(&self, name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.handlers.get(name).map(Arc::clone)
    }

    // TODO(jif) for dynamic tools.
    // pub fn register(&mut self, name: impl Into<String>, handler: Arc<dyn ToolHandler>) {
    //     let name = name.into();
    //     if self.handlers.insert(name.clone(), handler).is_some() {
    //         warn!("overwriting handler for tool {name}");
    //     }
    // }

    pub async fn dispatch<'a>(
        &self,
        invocation: ToolInvocation<'a>,
    ) -> Result<ResponseInputItem, FunctionCallError> {
        let tool_name = invocation.tool_name.clone();
        let handler = self.handler(tool_name.as_ref()).ok_or_else(|| {
            FunctionCallError::RespondToModel(format!("unsupported call: {tool_name}"))
        })?;

        if !handler.matches_kind(&invocation.payload) {
            return Err(FunctionCallError::RespondToModel(format!(
                "tool {tool_name} invoked with incompatible payload"
            )));
        }

        let call_id_owned = invocation.call_id.clone();
        let otel = invocation.turn.client.get_otel_event_manager();
        let log_payload = invocation.payload.log_payload().into_owned();
        let output_cell = std::sync::Mutex::new(None);

        let result = otel
            .log_tool_result(tool_name.as_ref(), &call_id_owned, &log_payload, || {
                let handler = handler.clone();
                let output_cell = &output_cell;
                let invocation = invocation;
                async move {
                    match handler.handle(invocation).await {
                        Ok(output) => {
                            let preview = output.log_preview();
                            let mut guard = output_cell
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            *guard = Some(output);
                            Ok(preview)
                        }
                        Err(err) => Err(err),
                    }
                }
            })
            .await;

        match result {
            Ok(_) => {
                let mut guard = output_cell
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let output = guard.take().ok_or_else(|| {
                    FunctionCallError::RespondToModel("tool produced no output".to_string())
                })?;
                Ok(output.into_response(&call_id_owned))
            }
            Err(err) => Err(err),
        }
    }
}

pub struct ToolRegistryBuilder {
    handlers: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register_handler(&mut self, name: impl Into<String>, handler: Arc<dyn ToolHandler>) {
        let name = name.into();
        if self
            .handlers
            .insert(name.clone(), handler.clone())
            .is_some()
        {
            warn!("overwriting handler for tool {name}");
        }
    }

    // TODO(jif) for dynamic tools.
    // pub fn register_many<I>(&mut self, names: I, handler: Arc<dyn ToolHandler>)
    // where
    //     I: IntoIterator,
    //     I::Item: Into<String>,
    // {
    //     for name in names {
    //         let name = name.into();
    //         if self
    //             .handlers
    //             .insert(name.clone(), handler.clone())
    //             .is_some()
    //         {
    //             warn!("overwriting handler for tool {name}");
    //         }
    //     }
    // }

    pub fn build(self) -> ToolRegistry {
        ToolRegistry::new(self.handlers)
    }
}
