mod common;

use std::path::Path;
use std::thread;
use std::time::Duration;

use codex_core::exec::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
use codex_core::protocol::ReviewDecision;
use codex_mcp_server::ExecApprovalElicitRequestParams;
use codex_mcp_server::ExecApprovalResponse;
use mcp_types::ElicitRequest;
use mcp_types::ElicitRequestParamsRequestedSchema;
use mcp_types::JSONRPC_VERSION;
use mcp_types::JSONRPCRequest;
use mcp_types::ModelContextProtocolRequest;
use mcp_types::RequestId;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

use crate::common::McpProcess;
use crate::common::create_final_assistant_message_sse_response;
use crate::common::create_mock_server;
use crate::common::create_shell_sse_response;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_shell_command_approval_triggers_elicitation() {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    // Apparently `#[tokio::test]` must return `()`, so we create a helper
    // function that returns `Result` so we can use `?` in favor of `unwrap`.
    if let Err(err) = shell_command_approval_triggers_elicitation().await {
        panic!("failure: {err}");
    }
}

async fn shell_command_approval_triggers_elicitation() -> anyhow::Result<()> {
    let workdir_for_shell_function_call = TempDir::new()?;

    let chat_completions_responses = vec![
        create_shell_sse_response(
            // We use `git init` because it will not be on the "trusted" list.
            vec!["git".to_string(), "init".to_string()],
            Some(workdir_for_shell_function_call.path()),
            Some(5_000),
        )?,
        create_final_assistant_message_sse_response("Enjoy your new git repo!")?,
    ];
    let server = create_mock_server(chat_completions_responses).await;

    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), server.uri())?;

    // TODO(mbolin): Introduce timeouts for individual MCP interactions.
    let mut mcp_process = McpProcess::new(codex_home.path())?;
    mcp_process.initialize()?;

    // Send "codex" tool request, which should hit the completions endpoint,
    // which should reply with a tool call, which the MCP should forward as an
    // elicitation.
    let request_id = mcp_process.send_codex_tool_call("run `git init`")?;
    let jsonrpc_request = mcp_process.read_stream_until_request_message()?;

    // This is the first request from the server, so the id should be 0 given
    // how things are currently implemented.
    let elicitation_request_id = RequestId::Integer(0);
    assert_eq!(
        JSONRPCRequest {
            jsonrpc: JSONRPC_VERSION.into(),
            id: elicitation_request_id.clone(),
            method: ElicitRequest::METHOD.to_string(),
            params: Some(serde_json::to_value(&ExecApprovalElicitRequestParams {
                message: format!(
                    "Allow Codex to run `git init` in \"{workdir}\"?",
                    workdir = workdir_for_shell_function_call.path().to_string_lossy()
                ),
                requested_schema: ElicitRequestParamsRequestedSchema {
                    r#type: "object".to_string(),
                    properties: json!({}),
                    required: None,
                },
                codex_elicitation: "exec-approval".to_string(),
                codex_mcp_tool_call_id: request_id.to_string(),
                // Internal Codex id: empirically it is 1, but this is
                // admittedly an internal detail that could change.
                codex_event_id: "1".to_string(),
                codex_command: vec!["git".into(), "init".into()],
                codex_cwd: workdir_for_shell_function_call.path().to_path_buf()
            })?)
        },
        jsonrpc_request
    );

    // Accept the `git init` request.
    mcp_process.send_response(
        elicitation_request_id,
        serde_json::to_value(ExecApprovalResponse {
            decision: ReviewDecision::Approved,
        })?,
    )?;

    thread::sleep(Duration::from_secs(5));

    assert!(
        workdir_for_shell_function_call.path().join(".git").is_dir(),
        ".git folder should have been created"
    );

    // TODO(mbolin): Verify the other responses that should have come back.

    Ok(())
}

fn create_config_toml(codex_home: &Path, server_uri: String) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"

model = "gpt-1000"
approval_policy = "untrusted"
sandbox_policy = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "chat"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}
