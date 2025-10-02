#![cfg(not(target_os = "windows"))]

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use codex_core::protocol::SandboxPolicy;
use codex_protocol::config_types::ReasoningSummary;
use core_test_support::responses;
use core_test_support::responses::ev_apply_patch_function_call;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use wiremock::matchers::any;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_only_tools_execute_before_apply_patch() -> anyhow::Result<()> {
    // Bail out early if the sandbox does not allow network traffic, because the
    // mocked Codex server still communicates over HTTP.
    skip_if_no_network!(Ok(()));

    // Stand up a mock Codex backend that will stream tool calls and responses.
    let server = start_mock_server().await;

    let mut builder = test_codex().with_config(|config| {
        config.enable_parallel_read_only_tools = true;
        config.include_apply_patch_tool = true;
    });
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    // Create two FIFOs that the mocked read-only tools will try to read from in
    // order to simulate long running, blocking I/O.
    let fifo_one = cwd.path().join("parallel_fifo_one");
    let fifo_two = cwd.path().join("parallel_fifo_two");
    create_fifo(&fifo_one)?;
    create_fifo(&fifo_two)?;

    let read_call_one = "read-file-1";
    let read_call_two = "read-file-2";
    let patch_call = "apply-patch";

    let read_args_one = serde_json::json!({
        "file_path": fifo_one.to_string_lossy(),
        "offset": 1,
        "limit": 1,
    })
    .to_string();
    let read_args_two = serde_json::json!({
        "file_path": fifo_two.to_string_lossy(),
        "offset": 1,
        "limit": 1,
    })
    .to_string();

    let patch_path = "parallel_patch_output.txt";
    let patch_content = format!(
        "*** Begin Patch\n*** Add File: {patch_path}\n+parallel apply_patch executed\n*** End Patch"
    );

    // Queue the first SSE response that drives the session: fire two read-only
    // tool calls, then schedule the apply-patch call.
    let first_response = sse(vec![
        serde_json::json!({
            "type": "response.created",
            "response": {"id": "resp-parallel"}
        }),
        ev_function_call(read_call_one, "read_file", &read_args_one),
        ev_function_call(read_call_two, "read_file", &read_args_two),
        ev_apply_patch_function_call(patch_call, &patch_content),
        ev_completed("resp-parallel"),
    ]);
    responses::mount_sse_once_match(&server, any(), first_response).await;

    // Queue a follow-up response so the session can complete once all tools run.
    let second_response = sse(vec![
        ev_assistant_message("msg-1", "all done"),
        ev_completed("resp-final"),
    ]);
    responses::mount_sse_once_match(&server, any(), second_response).await;

    // Start timers that resolve when each FIFO gets a writer, helping us measure
    // when the corresponding read-only tool begins execution.
    let start = Instant::now();
    let wait_one = tokio::spawn(wait_for_writer(fifo_one.clone(), start));
    let wait_two = tokio::spawn(wait_for_writer(fifo_two.clone(), start));

    // Capture all Codex events so we can verify tool ordering once execution finishes.
    let events = Arc::new(Mutex::new(Vec::new()));
    let (done_tx, done_rx) = oneshot::channel();
    let events_task = events.clone();
    let codex_for_events = codex.clone();
    tokio::spawn(async move {
        loop {
            let event = codex_for_events.next_event().await.expect("event");
            let msg = event.msg;
            let is_done = matches!(msg, EventMsg::TaskComplete(_));
            {
                let mut log = events_task.lock().await;
                log.push(msg);
            }
            if is_done {
                break;
            }
        }
        let _ = done_tx.send(());
    });

    let session_model = session_configured.model.clone();

    // Trigger the user turn that causes Codex to invoke the two read-only tools
    // and subsequently the apply-patch tool.
    codex
        .submit(Op::UserTurn {
            items: vec![InputItem::Text {
                text: "please process the tools in parallel".into(),
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

    let wait_timeout = Duration::from_secs(5);

    let (mut writer_one, elapsed_one) = tokio::time::timeout(wait_timeout, async {
        wait_one.await.expect("wait fifo one task panicked")
    })
    .await
    .expect("timeout waiting for first read-only tool")?;

    let (mut writer_two, elapsed_two) = tokio::time::timeout(wait_timeout, async {
        wait_two.await.expect("wait fifo two task panicked")
    })
    .await
    .expect("timeout waiting for second read-only tool")?;

    // Ensure the two read-only tools started within 200ms of each other so that
    // they can be considered parallel.
    let delta = if elapsed_one > elapsed_two {
        elapsed_one - elapsed_two
    } else {
        elapsed_two - elapsed_one
    };
    assert!(
        delta < Duration::from_millis(200),
        "expected read-only tools to start in parallel (delta {delta:?})"
    );

    writer_one.write_all(b"fifo one line\n").await?;
    writer_one.shutdown().await?;
    drop(writer_one);

    tokio::time::sleep(Duration::from_millis(100)).await;

    {
        // Confirm that apply_patch has not started while the second read-only
        // tool is still blocked waiting for input.
        let log = events.lock().await;
        assert!(
            !log.iter()
                .any(|msg| matches!(msg, EventMsg::PatchApplyBegin(_))),
            "apply_patch began before the second read-only tool completed"
        );
    }

    writer_two.write_all(b"fifo two line\n").await?;
    writer_two.shutdown().await?;
    drop(writer_two);

    // Wait for the event collector to observe task completion so we can inspect
    // the final event log.
    done_rx.await.expect("event collector finished");

    let events_log = events.lock().await;
    let patch_begin_index = events_log
        .iter()
        .position(|msg| match msg {
            EventMsg::PatchApplyBegin(begin) => begin.call_id == patch_call,
            _ => false,
        })
        .expect("expected PatchApplyBegin event");
    let patch_end_index = events_log
        .iter()
        .position(|msg| match msg {
            EventMsg::PatchApplyEnd(end) => end.call_id == patch_call,
            _ => false,
        })
        .expect("expected PatchApplyEnd event");
    assert!(
        patch_begin_index < patch_end_index,
        "PatchApplyEnd occurred before PatchApplyBegin"
    );

    // Record whether apply_patch succeeded so the assertions below can verify
    // either the patched file or the reported stderr output.
    let patch_end_success = events_log.iter().find_map(|msg| match msg {
        EventMsg::PatchApplyEnd(end) if end.call_id == patch_call => {
            Some((end.success, end.stderr.clone()))
        }
        _ => None,
    });
    let (patch_success, patch_stderr) = patch_end_success.expect("expected PatchApplyEnd details");
    drop(events_log);

    if patch_success {
        let patched_file = cwd.path().join(patch_path);
        let patched_contents = std::fs::read_to_string(&patched_file)?;
        assert!(
            patched_contents.contains("parallel apply_patch executed"),
            "unexpected patch contents: {patched_contents:?}"
        );
    } else {
        assert!(
            patch_stderr.contains("codex-run-as-apply-patch"),
            "unexpected apply_patch stderr: {patch_stderr:?}"
        );
    }

    // Check that the mock server observed outputs from every tool invocation.
    let requests = server.received_requests().await.expect("recorded requests");
    assert!(
        !requests.is_empty(),
        "expected at least one request recorded"
    );

    let mut seen_outputs = std::collections::HashSet::new();
    for request in requests {
        let body = request
            .body_json::<serde_json::Value>()
            .expect("request json");
        if let Some(items) = body.get("input").and_then(|v| v.as_array()) {
            for item in items {
                if item.get("type").and_then(|v| v.as_str()) == Some("function_call_output") {
                    if let Some(call_id) = item.get("call_id").and_then(|v| v.as_str()) {
                        seen_outputs.insert(call_id.to_string());
                    }
                }
            }
        }
    }

    assert!(
        seen_outputs.contains(read_call_one),
        "missing read-only tool output for {read_call_one}"
    );
    assert!(
        seen_outputs.contains(read_call_two),
        "missing read-only tool output for {read_call_two}"
    );
    assert!(
        seen_outputs.contains(patch_call),
        "missing apply_patch tool output"
    );

    Ok(())
}

fn create_fifo(path: &Path) -> anyhow::Result<()> {
    let c_path =
        CString::new(path.as_os_str().as_bytes()).context("fifo path contained null byte")?;
    let res = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    if res != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

async fn wait_for_writer(
    path: PathBuf,
    origin: Instant,
) -> anyhow::Result<(tokio::fs::File, Duration)> {
    let file = OpenOptions::new()
        .write(true)
        .open(&path)
        .await
        .with_context(|| format!("open fifo {:?} for writing", path))?;
    Ok((file, origin.elapsed()))
}
