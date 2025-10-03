use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use codex_protocol::models::ResponseInputItem;
use tracing::warn;

use crate::client_common::tools::ToolSpec;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolCapabilities {
    pub read_only: bool,
}

impl ToolCapabilities {
    pub const fn mutating() -> Self {
        Self { read_only: false }
    }

    pub const fn read_only() -> Self {
        Self { read_only: true }
    }
}

#[derive(Clone)]
struct ToolEntry {
    handler: Arc<dyn ToolHandler>,
    capabilities: ToolCapabilities,
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn kind(&self) -> ToolKind;

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            (self.kind(), payload),
            (ToolKind::Function, ToolPayload::Function { .. })
                | (ToolKind::UnifiedExec, ToolPayload::UnifiedExec { .. })
                | (ToolKind::Mcp, ToolPayload::Mcp { .. })
        )
    }

    async fn handle(&self, invocation: ToolInvocation<'_>)
    -> Result<ToolOutput, FunctionCallError>;
}

#[derive(Clone)]
pub struct ToolRegistry {
    handlers: HashMap<String, ToolEntry>,
}

impl ToolRegistry {
    fn new(handlers: HashMap<String, ToolEntry>) -> Self {
        Self { handlers }
    }

    pub fn capabilities(&self, name: &str) -> Option<ToolCapabilities> {
        self.handlers.get(name).map(|entry| entry.capabilities)
    }

    pub fn has_read_only_tools(&self) -> bool {
        self.handlers
            .values()
            .any(|entry| entry.capabilities.read_only)
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
        let call_id_owned = invocation.call_id.clone();
        let otel = invocation.turn.client.get_otel_event_manager();
        let payload_for_response = invocation.payload.clone();
        let log_payload = payload_for_response.log_payload();

        let entry = match self.handlers.get(tool_name.as_str()) {
            Some(entry) => entry,
            None => {
                let message =
                    unsupported_tool_call_message(&invocation.payload, tool_name.as_ref());
                otel.tool_result(
                    tool_name.as_ref(),
                    &call_id_owned,
                    log_payload.as_ref(),
                    Duration::ZERO,
                    false,
                    &message,
                );
                return Err(FunctionCallError::RespondToModel(message));
            }
        };

        let handler = Arc::clone(&entry.handler);

        if !handler.matches_kind(&invocation.payload) {
            let message = format!("tool {tool_name} invoked with incompatible payload");
            otel.tool_result(
                tool_name.as_ref(),
                &call_id_owned,
                log_payload.as_ref(),
                Duration::ZERO,
                false,
                &message,
            );
            return Err(FunctionCallError::Fatal(message));
        }

        let output_cell = tokio::sync::Mutex::new(None);

        let result = otel
            .log_tool_result(
                tool_name.as_ref(),
                &call_id_owned,
                log_payload.as_ref(),
                || {
                    let handler = handler.clone();
                    let output_cell = &output_cell;
                    let invocation = invocation;
                    async move {
                        match handler.handle(invocation).await {
                            Ok(output) => {
                                let preview = output.log_preview();
                                let success = output.success_for_logging();
                                let mut guard = output_cell.lock().await;
                                *guard = Some(output);
                                Ok((preview, success))
                            }
                            Err(err) => Err(err),
                        }
                    }
                },
            )
            .await;

        match result {
            Ok(_) => {
                let mut guard = output_cell.lock().await;
                let output = guard.take().ok_or_else(|| {
                    FunctionCallError::RespondToModel("tool produced no output".to_string())
                })?;
                Ok(output.into_response(&call_id_owned, &payload_for_response))
            }
            Err(err) => Err(err),
        }
    }
}

pub struct ToolRegistryBuilder {
    handlers: HashMap<String, ToolEntry>,
    specs: Vec<ToolSpec>,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            specs: Vec::new(),
        }
    }

    pub fn push_spec(&mut self, spec: ToolSpec) {
        self.specs.push(spec);
    }

    pub fn register_handler(&mut self, name: impl Into<String>, handler: Arc<dyn ToolHandler>) {
        self.register_with_capabilities(name, handler, ToolCapabilities::mutating());
    }

    pub fn register_read_only_handler(
        &mut self,
        name: impl Into<String>,
        handler: Arc<dyn ToolHandler>,
    ) {
        self.register_with_capabilities(name, handler, ToolCapabilities::read_only());
    }

    pub fn register_with_capabilities(
        &mut self,
        name: impl Into<String>,
        handler: Arc<dyn ToolHandler>,
        capabilities: ToolCapabilities,
    ) {
        let name = name.into();
        if self
            .handlers
            .insert(
                name.clone(),
                ToolEntry {
                    handler: handler.clone(),
                    capabilities,
                },
            )
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

    pub fn build(self) -> (Vec<ToolSpec>, ToolRegistry) {
        let registry = ToolRegistry::new(self.handlers);
        (self.specs, registry)
    }
}

fn unsupported_tool_call_message(payload: &ToolPayload, tool_name: &str) -> String {
    match payload {
        ToolPayload::Custom { .. } => format!("unsupported custom tool call: {tool_name}"),
        _ => format!("unsupported call: {tool_name}"),
    }
}
