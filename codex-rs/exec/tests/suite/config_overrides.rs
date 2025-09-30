#![cfg(not(target_os = "windows"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use core_test_support::responses;
use core_test_support::test_codex_exec::test_codex_exec;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_overrides_are_applied_resume() -> anyhow::Result<()> {
    config_overrides_are_applied_resume_inner(
        &["--model", "o3", "prompt text"],
        &[
            "resume",
            "--skip-git-repo-check",
            "fake id",
            "--model",
            "o3",
            "resume prompt text",
        ],
    )
    .await?;

    config_overrides_are_applied_resume_inner(
        &["-c", "model=o3", "prompt text"],
        &[
            "resume",
            "--skip-git-repo-check",
            "fake id",
            "-c",
            "model=o3",
            "resume prompt text",
        ],
    )
    .await?;

    config_overrides_are_applied_resume_inner(
        &["-c", "model=o3", "prompt text"],
        &[
            "-c",
            "model=o3",
            "resume",
            "--skip-git-repo-check",
            "fake id",
            "resume prompt text",
        ],
    )
    .await?;

    Ok(())
}

async fn config_overrides_are_applied_resume_inner(
    args: &[&str],
    resume_args: &[&str],
) -> anyhow::Result<()> {
    let test = test_codex_exec();

    let match_config_overrides_applied = |req: &wiremock::Request| {
        let body = std::str::from_utf8(&req.body).unwrap_or("");
        body.contains("prompt text") && body.contains("o3")
    };

    let server = responses::start_mock_server().await;
    responses::mount_sse_once_match(
        &server,
        match_config_overrides_applied,
        responses::sse(vec![
            responses::ev_assistant_message("msg-1", "config overrides are applied."),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    test.cmd_with_server(&server)
        .arg("--skip-git-repo-check")
        .arg("--experimental-json")
        .args(args)
        .assert()
        .code(0);

    let match_config_resume_overrides_applied = |req: &wiremock::Request| {
        let body = std::str::from_utf8(&req.body).unwrap_or("");
        body.contains("resume prompt text") && body.contains("o3")
    };

    responses::mount_sse_once_match(
        &server,
        match_config_resume_overrides_applied,
        responses::sse(vec![
            responses::ev_assistant_message("msg-1", "config overrides are applied resume."),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    test.cmd_with_server(&server)
        .args(resume_args)
        .assert()
        .code(0);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_overrides_are_applied() -> anyhow::Result<()> {
    config_overrides_are_applied_inner(&["-c", "model=o3", "prompt text"]).await?;
    config_overrides_are_applied_inner(&["prompt text", "-c", "model=o3"]).await?;
    config_overrides_are_applied_inner(&["--model", "o3", "prompt text"]).await?;

    Ok(())
}

async fn config_overrides_are_applied_inner(args: &[&str]) -> anyhow::Result<()> {
    let test = test_codex_exec();

    let match_config_overrides_applied = |req: &wiremock::Request| {
        let body = std::str::from_utf8(&req.body).unwrap_or("");
        body.contains("prompt text") && body.contains("o3")
    };

    let server = responses::start_mock_server().await;
    responses::mount_sse_once_match(
        &server,
        match_config_overrides_applied,
        responses::sse(vec![
            responses::ev_assistant_message("msg-1", "config overrides are applied."),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    test.cmd_with_server(&server)
        .arg("--skip-git-repo-check")
        .args(args)
        .assert()
        .code(0);

    Ok(())
}
