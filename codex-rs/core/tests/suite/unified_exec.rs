#![cfg(not(target_os = "windows"))]

use anyhow::Result;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use codex_core::protocol::SandboxPolicy;
use codex_protocol::config_types::ReasoningSummary;
use core_test_support::responses::ResponseMock;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::skip_if_sandbox;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use serde_json::Value;

fn extract_output_text(item: &Value) -> Option<&str> {
    item.get("output").and_then(|value| match value {
        Value::String(text) => Some(text.as_str()),
        Value::Object(obj) => obj.get("content").and_then(Value::as_str),
        _ => None,
    })
}

fn function_call_output_json(mock: &ResponseMock, call_id: &str) -> Result<Value> {
    let request = mock
        .requests()
        .into_iter()
        .find(|request| {
            request.input().iter().any(|item| {
                item.get("type").and_then(Value::as_str) == Some("function_call_output")
                    && item.get("call_id").and_then(Value::as_str) == Some(call_id)
            })
        })
        .ok_or_else(|| anyhow::anyhow!("missing {call_id} function_call_output"))?;
    let item = request.function_call_output(call_id);
    let content = extract_output_text(&item)
        .ok_or_else(|| anyhow::anyhow!("missing tool output content for {call_id}"))?;
    Ok(serde_json::from_str(content)?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_reuses_session_via_stdin() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_codex().with_config(|config| {
        config.use_experimental_unified_exec_tool = true;
    });
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let first_call_id = "uexec-start";
    let first_args = serde_json::json!({
        "input": ["/bin/cat"],
        "timeout_ms": 200,
    });

    let second_call_id = "uexec-stdin";
    let second_args = serde_json::json!({
        "input": ["hello unified exec\n"],
        "session_id": "0",
        "timeout_ms": 500,
    });

    let _first_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                first_call_id,
                "unified_exec",
                &serde_json::to_string(&first_args)?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let _second_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-2"),
            ev_function_call(
                second_call_id,
                "unified_exec",
                &serde_json::to_string(&second_args)?,
            ),
            ev_completed("resp-2"),
        ]),
    )
    .await;
    let final_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "all done"),
            ev_completed("resp-3"),
        ]),
    )
    .await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![InputItem::Text {
                text: "run unified exec".into(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: ReasoningSummary::Auto,
        })
        .await?;

    wait_for_event(&codex, |event| matches!(event, EventMsg::TaskComplete(_))).await;

    let start_output = function_call_output_json(&final_mock, first_call_id)?;
    let session_id = start_output["session_id"].as_str().unwrap_or_default();
    assert!(
        !session_id.is_empty(),
        "expected session id in first unified_exec response"
    );
    assert!(
        start_output["output"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );

    let reuse_output = function_call_output_json(&final_mock, second_call_id)?;
    assert_eq!(
        reuse_output["session_id"].as_str().unwrap_or_default(),
        session_id
    );
    let echoed = reuse_output["output"].as_str().unwrap_or_default();
    assert!(
        echoed.contains("hello unified exec"),
        "expected echoed output, got {echoed:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_streams_after_lagged_output() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_codex().with_config(|config| {
        config.use_experimental_unified_exec_tool = true;
    });
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let script = r#"python3 - <<'PY'
import sys
import time

chunk = b'x' * (1 << 20)
for _ in range(4):
    sys.stdout.buffer.write(chunk)
    sys.stdout.flush()

time.sleep(0.2)
for _ in range(5):
    sys.stdout.write("TAIL-MARKER\n")
    sys.stdout.flush()
    time.sleep(0.05)

time.sleep(0.2)
PY
"#;

    let first_call_id = "uexec-lag-start";
    let first_args = serde_json::json!({
        "input": ["/bin/sh", "-c", script],
        "timeout_ms": 25,
    });

    let second_call_id = "uexec-lag-poll";
    let second_args = serde_json::json!({
        "input": Vec::<String>::new(),
        "session_id": "0",
        "timeout_ms": 800,
    });

    let _first_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                first_call_id,
                "unified_exec",
                &serde_json::to_string(&first_args)?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let _second_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-2"),
            ev_function_call(
                second_call_id,
                "unified_exec",
                &serde_json::to_string(&second_args)?,
            ),
            ev_completed("resp-2"),
        ]),
    )
    .await;
    let final_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "lag handled"),
            ev_completed("resp-3"),
        ]),
    )
    .await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![InputItem::Text {
                text: "exercise lag handling".into(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: ReasoningSummary::Auto,
        })
        .await?;

    wait_for_event(&codex, |event| matches!(event, EventMsg::TaskComplete(_))).await;

    let start_output = function_call_output_json(&final_mock, first_call_id)?;
    let session_id = start_output["session_id"].as_str().unwrap_or_default();
    assert!(
        !session_id.is_empty(),
        "expected session id from initial unified_exec response"
    );

    let poll_output = function_call_output_json(&final_mock, second_call_id)?;
    let poll_text = poll_output["output"].as_str().unwrap_or_default();
    assert!(
        poll_text.contains("TAIL-MARKER"),
        "expected poll output to contain tail marker, got {poll_text:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_timeout_and_followup_poll() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_codex().with_config(|config| {
        config.use_experimental_unified_exec_tool = true;
    });
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let first_call_id = "uexec-timeout";
    let first_args = serde_json::json!({
        "input": ["/bin/sh", "-c", "sleep 0.1; echo ready"],
        "timeout_ms": 10,
    });

    let second_call_id = "uexec-poll";
    let second_args = serde_json::json!({
        "input": Vec::<String>::new(),
        "session_id": "0",
        "timeout_ms": 800,
    });

    let _first_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                first_call_id,
                "unified_exec",
                &serde_json::to_string(&first_args)?,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let _second_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-2"),
            ev_function_call(
                second_call_id,
                "unified_exec",
                &serde_json::to_string(&second_args)?,
            ),
            ev_completed("resp-2"),
        ]),
    )
    .await;
    let final_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-3"),
        ]),
    )
    .await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![InputItem::Text {
                text: "check timeout".into(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: ReasoningSummary::Auto,
        })
        .await?;

    loop {
        let event = codex.next_event().await.expect("event");
        if matches!(event.msg, EventMsg::TaskComplete(_)) {
            break;
        }
    }

    let first_output = function_call_output_json(&final_mock, first_call_id)?;
    assert_eq!(first_output["session_id"], "0");
    assert!(
        first_output["output"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );

    let poll_output = function_call_output_json(&final_mock, second_call_id)?;
    let output_text = poll_output["output"].as_str().unwrap_or_default();
    assert!(
        output_text.contains("ready"),
        "expected ready output, got {output_text:?}"
    );

    Ok(())
}
