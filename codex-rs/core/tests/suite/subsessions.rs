use anyhow::Result;
use codex_core::model_family::find_family_for_model;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use codex_core::protocol::SandboxPolicy;
use codex_protocol::config_types::ReasoningSummary;
use core_test_support::responses;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use serde_json::Value;
use tokio::test;
use wiremock::matchers::any;

async fn collect_tool_names(model: &str) -> Result<Vec<String>> {
    let server = start_mock_server().await;
    let model_owned = model.to_string();
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = test_codex()
        .with_config(move |config| {
            config.model = model_owned.clone();
            config.model_family =
                find_family_for_model(&model_owned).expect("model family available for test");
        })
        .build(&server)
        .await?;

    let response = sse(vec![
        ev_response_created("resp-1"),
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-1"),
    ]);
    responses::mount_sse_once_match(&server, any(), response).await;

    codex
        .submit(Op::UserTurn {
            items: vec![InputItem::Text {
                text: "ping".into(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_configured.model.clone(),
            effort: None,
            summary: ReasoningSummary::Auto,
        })
        .await?;

    wait_for_event(&codex, |event| matches!(event, EventMsg::TaskComplete(_))).await;
    let requests = server.received_requests().await.expect("recorded requests");
    let first_body = requests
        .first()
        .ok_or_else(|| anyhow::anyhow!("expected at least one request"))?
        .body_json::<Value>()?;
    let tool_names = first_body
        .get("tools")
        .and_then(|tools| tools.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(tool_names)
}

#[test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_models_expose_subsession_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let tools = collect_tool_names("codex-mini-latest").await?;
    assert!(
        tools.contains(&"create_session".to_string())
            && tools.contains(&"wait_session".to_string())
            && tools.contains(&"cancel_session".to_string()),
        "expected subsession tool trio in {tools:?}"
    );
    Ok(())
}

#[test(flavor = "multi_thread", worker_threads = 2)]
async fn test_models_expose_subsession_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let tools = collect_tool_names("test-gpt-5-codex").await?;
    assert!(
        tools.contains(&"create_session".to_string())
            && tools.contains(&"wait_session".to_string())
            && tools.contains(&"cancel_session".to_string()),
        "expected subsession tool trio in {tools:?}"
    );
    Ok(())
}

#[test(flavor = "multi_thread", worker_threads = 2)]
async fn gpt5_codex_models_do_not_expose_subsession_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let tools = collect_tool_names("gpt-5-codex").await?;
    assert!(
        !tools.contains(&"create_session".to_string()),
        "unexpected subsession tools in {tools:?}"
    );
    Ok(())
}
