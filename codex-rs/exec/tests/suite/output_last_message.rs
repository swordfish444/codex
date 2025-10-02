#![cfg(not(target_os = "windows"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use core_test_support::responses;
use core_test_support::test_codex_exec::test_codex_exec;
use wiremock::matchers::any;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_includes_output_last_message_in_request() -> anyhow::Result<()> {
    let test = test_codex_exec();

    let last_message_path = test.cwd_path().join("last_message.txt");
    let server = responses::start_mock_server().await;
    let body = responses::sse(vec![
        serde_json::json!({
            "type": "response.created",
            "response": {"id": "resp1"}
        }),
        responses::ev_assistant_message("m1", "fixture hello"),
        responses::ev_completed("resp1"),
    ]);
    responses::mount_sse_once_match(&server, any(), body).await;

    test.cmd_with_server(&server)
        .arg("--skip-git-repo-check")
        // keep using -C in the test to exercise the flag as well
        .arg("-C")
        .arg(test.cwd_path())
        .arg("--output-last-message")
        .arg(&last_message_path)
        .arg("tell me a joke")
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(&last_message_path)?,
        "fixture hello"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_includes_output_last_message_in_request_json() -> anyhow::Result<()> {
    let test = test_codex_exec();

    let last_message_path = test.cwd_path().join("last_message.txt");
    let server = responses::start_mock_server().await;
    let body = responses::sse(vec![
        serde_json::json!({
            "type": "response.created",
            "response": {"id": "resp1"}
        }),
        responses::ev_assistant_message("m1", "fixture hello"),
        responses::ev_completed("resp1"),
    ]);
    responses::mount_sse_once_match(&server, any(), body).await;

    test.cmd_with_server(&server)
        .arg("--skip-git-repo-check")
        // keep using -C in the test to exercise the flag as well
        .arg("-C")
        .arg(test.cwd_path())
        .arg("--output-last-message")
        .arg(&last_message_path)
        .arg("--json")
        .arg("tell me a joke")
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(&last_message_path)?,
        "fixture hello"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_includes_output_last_message_in_request_stdout() -> anyhow::Result<()> {
    let test = test_codex_exec();

    let server = responses::start_mock_server().await;
    let body = responses::sse(vec![
        serde_json::json!({
            "type": "response.created",
            "response": {"id": "resp1"}
        }),
        responses::ev_assistant_message("m1", "fixture hello"),
        responses::ev_completed("resp1"),
    ]);
    responses::mount_sse_once_match(&server, any(), body).await;

    test.cmd_with_server(&server)
        .arg("--skip-git-repo-check")
        // keep using -C in the test to exercise the flag as well
        .arg("-C")
        .arg(test.cwd_path())
        .arg("tell me a joke")
        .arg("--output-last-message")
        .assert()
        .success()
        .stdout("fixture hello\n");

    Ok(())
}
