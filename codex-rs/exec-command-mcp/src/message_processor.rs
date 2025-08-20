use std::sync::Arc;

use mcp_types::CallToolRequestParams;
use mcp_types::CallToolResult;
use mcp_types::ClientRequest as McpClientRequest;
use mcp_types::ContentBlock;
use mcp_types::JSONRPCErrorError;
use mcp_types::JSONRPCRequest;
use mcp_types::ListToolsResult;
use mcp_types::ModelContextProtocolRequest;
use mcp_types::RequestId;
use mcp_types::ServerCapabilitiesTools;
use mcp_types::TextContent;
use mcp_types::Tool;
use mcp_types::ToolInputSchema;
use schemars::r#gen::SchemaSettings;

use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::error_code::{self};
use crate::exec_command::ExecCommandParams;
use crate::exec_command::WriteStdinParams;
use crate::outgoing_message_sender::OutgoingMessageSender;
use crate::session_manager::SessionManager;

#[derive(Debug)]
pub(crate) struct MessageProcessor {
    initialized: bool,
    outgoing: Arc<OutgoingMessageSender>,
    session_manager: Arc<SessionManager>,
}

impl MessageProcessor {
    pub(crate) fn new(outgoing: OutgoingMessageSender) -> Self {
        Self {
            initialized: false,
            outgoing: Arc::new(outgoing),
            session_manager: Arc::new(SessionManager::default()),
        }
    }

    pub(crate) async fn process_request(&mut self, request: JSONRPCRequest) {
        let request_id = request.id.clone();
        let client_request = match McpClientRequest::try_from(request) {
            Ok(client_request) => client_request,
            Err(e) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: error_code::INVALID_REQUEST_ERROR_CODE,
                            message: format!("Invalid request: {e}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        match client_request {
            McpClientRequest::InitializeRequest(params) => {
                self.handle_initialize(request_id, params).await;
            }
            McpClientRequest::ListToolsRequest(params) => {
                self.handle_list_tools(request_id, params).await;
            }
            McpClientRequest::CallToolRequest(params) => {
                self.handle_call_tool(request_id, params).await;
            }
            _ => {
                tracing::warn!("Unhandled client request: {client_request:?}");
            }
        }
    }

    async fn handle_initialize(
        &mut self,
        id: RequestId,
        params: <mcp_types::InitializeRequest as ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("initialize -> params: {:?}", params);

        if self.initialized {
            // Already initialised: send JSON-RPC error response.
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "initialize called more than once".to_string(),
                data: None,
            };
            self.outgoing.send_error(id, error).await;
            return;
        }

        self.initialized = true;

        // Build a minimal InitializeResult. Fill with placeholders.
        let result = mcp_types::InitializeResult {
            capabilities: mcp_types::ServerCapabilities {
                completions: None,
                experimental: None,
                logging: None,
                prompts: None,
                resources: None,
                tools: Some(ServerCapabilitiesTools {
                    list_changed: Some(true),
                }),
            },
            instructions: None,
            protocol_version: params.protocol_version.clone(),
            server_info: mcp_types::Implementation {
                name: "exec-command-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Codex exec_command".to_string()),
            },
        };

        self.send_response::<mcp_types::InitializeRequest>(id, result)
            .await;
    }

    async fn handle_list_tools(
        &self,
        request_id: RequestId,
        params: <mcp_types::ListToolsRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::trace!("tools/list ({request_id:?}) -> {params:?}");

        // Generate tool schema eagerly in a short-lived scope to avoid holding
        // non-Send schemars generator across await.
        let result = {
            let generator = SchemaSettings::draft2019_09()
                .with(|s| {
                    s.inline_subschemas = true;
                    s.option_add_null_type = false;
                })
                .into_generator();

            let exec_schema = generator
                .clone()
                .into_root_schema_for::<ExecCommandParams>();
            let write_schema = generator.into_root_schema_for::<WriteStdinParams>();

            #[expect(clippy::expect_used)]
            let exec_schema_json =
                serde_json::to_value(&exec_schema).expect("exec_command schema should serialize");
            #[expect(clippy::expect_used)]
            let write_schema_json =
                serde_json::to_value(&write_schema).expect("write_stdin schema should serialize");

            let exec_input_schema = serde_json::from_value::<ToolInputSchema>(exec_schema_json)
                .unwrap_or_else(|e| {
                    panic!("failed to create Tool from schema: {e}");
                });
            let write_input_schema = serde_json::from_value::<ToolInputSchema>(write_schema_json)
                .unwrap_or_else(|e| {
                    panic!("failed to create Tool from schema: {e}");
                });

            let tools = vec![
                Tool {
                    name: "functions.exec_command".to_string(),
                    title: Some("Exec Command".to_string()),
                    description: Some("Start a PTY-backed shell command; returns early on timeout or completion.".to_string()),
                    input_schema: exec_input_schema,
                    output_schema: None,
                    annotations: None,
                },
                Tool {
                    name: "functions.write_stdin".to_string(),
                    title: Some("Write Stdin".to_string()),
                    description: Some("Write characters to a running exec session and collect output for a short window.".to_string()),
                    input_schema: write_input_schema,
                    output_schema: None,
                    annotations: None,
                },
            ];

            ListToolsResult {
                tools,
                next_cursor: None,
            }
        };

        self.send_response::<mcp_types::ListToolsRequest>(request_id, result)
            .await;
    }

    async fn handle_call_tool(
        &self,
        request_id: RequestId,
        params: <mcp_types::CallToolRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("tools/call -> params: {params:?}");
        let CallToolRequestParams { name, arguments } = params;

        match name.as_str() {
            "functions.exec_command" => match extract_exec_command_params(arguments).await {
                Ok(params) => {
                    tracing::info!("functions.exec_command -> params: {params:?}");
                    let session_manager = self.session_manager.clone();
                    let outgoing = self.outgoing.clone();
                    tokio::spawn(async move {
                        session_manager
                            .handle_exec_command_request(request_id, params, outgoing)
                            .await;
                    });
                }
                Err(jsonrpc_error) => {
                    self.outgoing.send_error(request_id, jsonrpc_error).await;
                }
            },
            "functions.write_stdin" => match extract_write_stdin_params(arguments).await {
                Ok(params) => {
                    tracing::info!("functions.write_stdin -> params: {params:?}");
                    let session_manager = self.session_manager.clone();
                    let outgoing = self.outgoing.clone();
                    tokio::spawn(async move {
                        session_manager
                            .handle_write_stdin_request(request_id, params, outgoing)
                            .await;
                    });
                }
                Err(jsonrpc_error) => {
                    self.outgoing.send_error(request_id, jsonrpc_error).await;
                }
            },
            _ => {
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text: format!("Unknown tool '{name}'"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.send_response::<mcp_types::CallToolRequest>(request_id, result)
                    .await;
            }
        }
    }

    async fn send_response<T>(&self, id: RequestId, result: T::Result)
    where
        T: ModelContextProtocolRequest,
    {
        self.outgoing.send_response(id, result).await;
    }
}

async fn extract_exec_command_params(
    args: Option<serde_json::Value>,
) -> Result<ExecCommandParams, JSONRPCErrorError> {
    match args {
        Some(value) => match serde_json::from_value::<ExecCommandParams>(value) {
            Ok(params) => Ok(params),
            Err(e) => Err(JSONRPCErrorError {
                code: error_code::INVALID_REQUEST_ERROR_CODE,
                message: format!("Invalid request: {e}"),
                data: None,
            }),
        },
        None => Err(JSONRPCErrorError {
            code: error_code::INVALID_REQUEST_ERROR_CODE,
            message: "Missing arguments".to_string(),
            data: None,
        }),
    }
}

async fn extract_write_stdin_params(
    args: Option<serde_json::Value>,
) -> Result<WriteStdinParams, JSONRPCErrorError> {
    match args {
        Some(value) => match serde_json::from_value::<WriteStdinParams>(value) {
            Ok(params) => Ok(params),
            Err(e) => Err(JSONRPCErrorError {
                code: error_code::INVALID_REQUEST_ERROR_CODE,
                message: format!("Invalid request: {e}"),
                data: None,
            }),
        },
        None => Err(JSONRPCErrorError {
            code: error_code::INVALID_REQUEST_ERROR_CODE,
            message: "Missing arguments".to_string(),
            data: None,
        }),
    }
}
