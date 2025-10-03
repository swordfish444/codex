#![cfg(not(target_os = "windows"))]
#![allow(clippy::unwrap_used)]
#![allow(clippy::await_holding_lock)]

use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use codex_core::protocol::AskForApproval;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use codex_core::protocol::SandboxPolicy;
use codex_protocol::config_types::ReasoningSummary;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use serde_json::json;

fn env_lock() -> &'static Mutex<()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

struct ReadFileDelayGuard {
    previous: Option<String>,
}

impl ReadFileDelayGuard {
    fn set(entries: &[(&std::path::Path, u64)]) -> Self {
        let previous = std::env::var("CODEX_TEST_READ_FILE_DELAYS").ok();
        if entries.is_empty() {
            remove_delay_env();
        } else {
            let combined = entries
                .iter()
                .map(|(path, delay)| format!("{}={delay}", path.display()))
                .collect::<Vec<_>>()
                .join(";");
            set_delay_env(&combined);
        }
        Self { previous }
    }
}

impl Drop for ReadFileDelayGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => set_delay_env(value),
            None => remove_delay_env(),
        }
    }
}

fn set_delay_env(value: &str) {
    unsafe { std::env::set_var("CODEX_TEST_READ_FILE_DELAYS", value) };
}

fn remove_delay_env() {
    unsafe { std::env::remove_var("CODEX_TEST_READ_FILE_DELAYS") };
}

async fn run_turn(test: &TestCodex, prompt: &str) -> anyhow::Result<()> {
    let session_model = test.session_configured.model.clone();

    test.codex
        .submit(Op::UserTurn {
            items: vec![InputItem::Text {
                text: prompt.into(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: ReasoningSummary::Auto,
        })
        .await?;

    loop {
        let event = test.codex.next_event().await?;
        if matches!(event.msg, EventMsg::TaskComplete(_)) {
            break;
        }
    }

    Ok(())
}

async fn run_turn_and_measure(test: &TestCodex, prompt: &str) -> anyhow::Result<Duration> {
    let start = Instant::now();
    run_turn(test, prompt).await?;
    Ok(start.elapsed())
}

fn assert_parallel_duration(actual: Duration) {
    assert!(
        actual < Duration::from_millis(500),
        "expected parallel execution to finish quickly, got {actual:?}"
    );
}

fn assert_serial_duration(actual: Duration) {
    assert!(
        actual >= Duration::from_millis(500),
        "expected serial execution to take longer, got {actual:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_file_tools_run_in_parallel() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let _lock = env_lock().lock().unwrap();

    let server = start_mock_server().await;
    let test = test_codex().build(&server).await?;

    let file_one = test.cwd.path().join("parallel_one.txt");
    let file_two = test.cwd.path().join("parallel_two.txt");
    std::fs::write(&file_one, "alpha\nbeta\n")?;
    std::fs::write(&file_two, "one\ntwo\n")?;

    let _guard = ReadFileDelayGuard::set(&[(&file_one, 300), (&file_two, 300)]);

    let args_one = json!({
        "file_path": file_one.to_string_lossy(),
        "offset": 1,
        "limit": 1,
    })
    .to_string();
    let args_two = json!({
        "file_path": file_two.to_string_lossy(),
        "offset": 1,
        "limit": 1,
    })
    .to_string();

    let first_response = sse(vec![
        json!({"type": "response.created", "response": {"id": "resp-1"}}),
        ev_function_call("call-1", "read_file", &args_one),
        ev_function_call("call-2", "read_file", &args_two),
        ev_completed("resp-1"),
    ]);
    let second_response = sse(vec![
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);
    mount_sse_sequence(&server, vec![first_response, second_response]).await;

    let duration = run_turn_and_measure(&test, "read the files").await?;
    assert_parallel_duration(duration);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_parallel_tools_run_serially() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let _lock = env_lock().lock().unwrap();

    let server = start_mock_server().await;
    let test = test_codex().build(&server).await?;

    let shell_args = json!({
        "command": ["/bin/sh", "-c", "sleep 0.3"],
        "timeout_ms": 1_000,
    });
    let args_one = serde_json::to_string(&shell_args)?;
    let args_two = serde_json::to_string(&shell_args)?;

    let first_response = sse(vec![
        json!({"type": "response.created", "response": {"id": "resp-1"}}),
        ev_function_call("call-1", "shell", &args_one),
        ev_function_call("call-2", "shell", &args_two),
        ev_completed("resp-1"),
    ]);
    let second_response = sse(vec![
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);
    mount_sse_sequence(&server, vec![first_response, second_response]).await;

    let duration = run_turn_and_measure(&test, "run shell twice").await?;
    assert_serial_duration(duration);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mixed_tools_fall_back_to_serial() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let _lock = env_lock().lock().unwrap();

    let server = start_mock_server().await;
    let test = test_codex().build(&server).await?;

    let file_path = test.cwd.path().join("mixed_file.txt");
    std::fs::write(&file_path, "zig\nzag\n")?;

    let _guard = ReadFileDelayGuard::set(&[(&file_path, 300)]);

    let read_args = json!({
        "file_path": file_path.to_string_lossy(),
        "offset": 1,
        "limit": 1,
    })
    .to_string();
    let shell_args = serde_json::to_string(&json!({
        "command": ["/bin/sh", "-c", "sleep 0.3"],
        "timeout_ms": 1_000,
    }))?;

    let first_response = sse(vec![
        json!({"type": "response.created", "response": {"id": "resp-1"}}),
        ev_function_call("call-1", "read_file", &read_args),
        ev_function_call("call-2", "shell", &shell_args),
        ev_completed("resp-1"),
    ]);
    let second_response = sse(vec![
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);
    mount_sse_sequence(&server, vec![first_response, second_response]).await;

    let duration = run_turn_and_measure(&test, "mix tools").await?;
    assert_serial_duration(duration);

    Ok(())
}
