use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Stdio;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use anyhow::Context;
use assert_cmd::prelude::*;
use codex_mcp_server::CodexToolCallParam;
use mcp_types::CallToolRequestParams;
use mcp_types::ClientCapabilities;
use mcp_types::Implementation;
use mcp_types::InitializeRequestParams;
use mcp_types::JSONRPC_VERSION;
use mcp_types::JSONRPCMessage;
use mcp_types::JSONRPCNotification;
use mcp_types::JSONRPCRequest;
use mcp_types::JSONRPCResponse;
use mcp_types::ModelContextProtocolNotification;
use mcp_types::ModelContextProtocolRequest;
use mcp_types::RequestId;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::io::Write;
use std::process::Command;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

pub fn create_shell_sse_response(
    command: Vec<String>,
    workdir: Option<&Path>,
    timeout_ms: Option<u64>,
) -> anyhow::Result<String> {
    // The `arguments`` for the `shell` tool is a serialized JSON object.
    let tool_call_arguments = serde_json::to_string(&json!({
        "command": command,
        "workdir": workdir.map(|w| w.to_string_lossy()),
        "timeout": timeout_ms
    }))?;
    let tool_call = json!({
        "choices": [
            {
                "delta": {
                    "tool_calls": [
                        {
                            "id": "call1234",
                            "function": {
                                "name": "shell",
                                "arguments": tool_call_arguments
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }
        ]
    });

    let sse = format!(
        "data: {}\n\ndata: DONE\n\n",
        serde_json::to_string(&tool_call)?
    );
    Ok(sse)
}

pub fn create_final_assistant_message_sse_response(message: &str) -> anyhow::Result<String> {
    let assistant_message = json!({
        "choices": [
            {
                "delta": {
                    "content": message
                },
                "finish_reason": "stop"
            }
        ]
    });

    let sse = format!(
        "data: {}\n\ndata: DONE\n\n",
        serde_json::to_string(&assistant_message)?
    );
    Ok(sse)
}

pub struct McpProcess {
    next_request_id: AtomicI64,
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl McpProcess {
    pub fn new(codex_home: &Path) -> anyhow::Result<Self> {
        let mut cmd = Command::cargo_bin("codex-mcp-server")
            .context("should find binary for codex-mcp-server")?;
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.env("CODEX_HOME", codex_home);
        cmd.env("RUST_LOG", "debug");

        let mut process = cmd.spawn().context("codex-mcp-server proc should start")?;
        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow::format_err!("mcp should have stdin fd"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::format_err!("mcp should have stdout fd"))?;
        let stdout = BufReader::new(stdout);
        Ok(Self {
            next_request_id: AtomicI64::new(0),
            process,
            stdin,
            stdout,
        })
    }

    /// Performs the initialization handshake with the MCP server.
    pub fn initialize(&mut self) -> anyhow::Result<()> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        let params = InitializeRequestParams {
            capabilities: ClientCapabilities {
                elicitation: Some(json!({})),
                experimental: None,
                roots: None,
                sampling: None,
            },
            client_info: Implementation {
                name: "elicitation test".into(),
                title: Some("Elicitation Test".into()),
                version: "0.0.0".into(),
            },
            protocol_version: mcp_types::MCP_SCHEMA_VERSION.into(),
        };
        let params_value = serde_json::to_value(params)?;

        self.send_jsonrpc_message(JSONRPCMessage::Request(JSONRPCRequest {
            jsonrpc: JSONRPC_VERSION.into(),
            id: RequestId::Integer(request_id),
            method: mcp_types::InitializeRequest::METHOD.into(),
            params: Some(params_value),
        }))?;

        let initialized = self.read_jsonrpc_message()?;
        assert_eq!(
            JSONRPCMessage::Response(JSONRPCResponse {
                jsonrpc: JSONRPC_VERSION.into(),
                id: RequestId::Integer(request_id),
                result: json!({
                    "capabilities": {
                        "tools": {
                            "listChanged": true
                        },
                    },
                    "serverInfo": {
                        "name": "codex-mcp-server",
                        "title": "Codex",
                        "version": "0.0.0"
                    },
                    "protocolVersion": mcp_types::MCP_SCHEMA_VERSION
                })
            }),
            initialized
        );

        // Send notifications/initialized to ack the response.
        self.send_jsonrpc_message(JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: JSONRPC_VERSION.into(),
            method: mcp_types::InitializedNotification::METHOD.into(),
            params: None,
        }))?;

        Ok(())
    }

    /// Returns the id used to make the request so it can be used when
    /// correlating notifications.
    pub fn send_codex_tool_call(&mut self, prompt: &str) -> anyhow::Result<i64> {
        let codex_tool_call_params = CallToolRequestParams {
            name: "codex".to_string(),
            arguments: Some(serde_json::to_value(CodexToolCallParam {
                prompt: prompt.to_string(),
                model: None,
                profile: None,
                cwd: None,
                approval_policy: None,
                sandbox: None,
                config: None,
            })?),
        };
        self.send_request(
            mcp_types::CallToolRequest::METHOD,
            Some(serde_json::to_value(codex_tool_call_params)?),
        )
    }

    fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> anyhow::Result<i64> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        let message = JSONRPCMessage::Request(JSONRPCRequest {
            jsonrpc: JSONRPC_VERSION.into(),
            id: RequestId::Integer(request_id),
            method: method.to_string(),
            params,
        });
        self.send_jsonrpc_message(message)?;
        Ok(request_id)
    }

    pub fn send_response(
        &mut self,
        id: RequestId,
        result: serde_json::Value,
    ) -> anyhow::Result<()> {
        self.send_jsonrpc_message(JSONRPCMessage::Response(JSONRPCResponse {
            jsonrpc: JSONRPC_VERSION.into(),
            id,
            result,
        }))
    }

    fn send_jsonrpc_message(&mut self, message: JSONRPCMessage) -> anyhow::Result<()> {
        let payload = serde_json::to_string(&message)?;
        self.stdin.write_all(payload.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn read_jsonrpc_message(&mut self) -> anyhow::Result<JSONRPCMessage> {
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let message = serde_json::from_str::<JSONRPCMessage>(&line)?;
        Ok(message)
    }

    pub fn read_stream_until_request_message(&mut self) -> anyhow::Result<JSONRPCRequest> {
        loop {
            let message = self.read_jsonrpc_message()?;
            eprint!("message: {message:?}");

            match message {
                JSONRPCMessage::Notification(_) => {
                    eprintln!("notification: {message:?}");
                }
                JSONRPCMessage::Request(jsonrpc_request) => {
                    return Ok(jsonrpc_request);
                }
                JSONRPCMessage::Error(_) => {
                    anyhow::bail!("unexpected JSONRPCMessage::Error: {message:?}");
                }
                JSONRPCMessage::Response(_) => {
                    anyhow::bail!("unexpected JSONRPCMessage::Response: {message:?}");
                }
            }
        }
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

pub async fn create_mock_server(responses: Vec<String>) -> MockServer {
    let server = MockServer::start().await;

    let num_calls = responses.len();
    let seq_responder = SeqResponder {
        num_calls: AtomicUsize::new(0),
        responses,
    };

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(seq_responder)
        .expect(num_calls as u64)
        .mount(&server)
        .await;

    server
}

struct SeqResponder {
    num_calls: AtomicUsize,
    responses: Vec<String>,
}

impl Respond for SeqResponder {
    fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
        let call_num = self.num_calls.fetch_add(1, Ordering::SeqCst);
        match self.responses.get(call_num) {
            Some(response) => ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(response.clone(), "text/event-stream"),
            None => panic!("no response for {call_num}"),
        }
    }
}
