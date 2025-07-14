#![allow(clippy::unwrap_used)]

//! End-to-end integration tests for the `codex` CLI.
//!
//! These spin up a local [`wiremock`][] server to stand in for the MCP server
//! and then run the real compiled `codex` binary against it. The goal is to
//! verify the high-level request/response flow rather than the details of the
//! individual async functions.
//!
//! [`wiremock`]: https://docs.rs/wiremock

use codex_core::exec::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ----- tests -----

/// Sends a single simple prompt and verifies that the streamed response is
/// surfaced to the user. This exercises the most common "ask a question, get a
/// textual answer" flow.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_conversation_turn_integration() {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!("Skipping test because network is disabled");
        return;
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(sse_message("Hello, world."), "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;

    // Disable retries — the mock server will fail hard if we make an unexpected
    // request, so retries only slow the test down.
    unsafe {
        std::env::set_var("OPENAI_REQUEST_MAX_RETRIES", "0");
        std::env::set_var("OPENAI_STREAM_MAX_RETRIES", "0");
    }

    let codex_home = TempDir::new().unwrap();
    let sandbox = TempDir::new().unwrap();
    write_config(codex_home.path(), &server);

    let mut cmd = assert_cmd::Command::cargo_bin("codex").unwrap();
    cmd.env("CODEX_HOME", codex_home.path())
        .current_dir(sandbox.path())
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("Hello");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Hello, world."));
}

/// Simulates a tool invocation (`shell`) followed by a second assistant message
/// once the tool call completes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_invocation_flow() {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!("Skipping test because network is disabled");
        return;
    }

    let server = MockServer::start().await;

    // The first request returns a function-call item; the second returns the
    // final assistant message. Use an atomic counter to serve them in order.
    struct SeqResponder {
        count: std::sync::atomic::AtomicUsize,
    }
    impl wiremock::Respond for SeqResponder {
        fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
            use std::sync::atomic::Ordering;
            match self.count.fetch_add(1, Ordering::SeqCst) {
                0 => ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(sse_function_call(), "text/event-stream"),
                _ => ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(sse_final_after_call(), "text/event-stream"),
            }
        }
    }

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(SeqResponder {
            count: std::sync::atomic::AtomicUsize::new(0),
        })
        .expect(2)
        .mount(&server)
        .await;

    unsafe {
        std::env::set_var("OPENAI_REQUEST_MAX_RETRIES", "0");
        std::env::set_var("OPENAI_STREAM_MAX_RETRIES", "0");
    }

    let codex_home = TempDir::new().unwrap();
    let sandbox = TempDir::new().unwrap();
    write_config(codex_home.path(), &server);

    let mut cmd = assert_cmd::Command::cargo_bin("codex").unwrap();
    cmd.env("CODEX_HOME", codex_home.path())
        .current_dir(sandbox.path())
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("Run shell");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("exec echo hi"))
        .stdout(predicate::str::contains("hi"));
}

// ----- helpers (keep below the tests) -----

/// Write a minimal `config.toml` pointing the CLI at the mock server.
fn write_config(codex_home: &Path, server: &MockServer) {
    fs::write(
        codex_home.join("config.toml"),
        format!(
            r#"
model_provider = "mock"
model = "test-model"

[model_providers.mock]
name = "mock"
base_url = "{}/v1"
env_key = "PATH"
wire_api = "responses"
"#,
            server.uri()
        ),
    )
    .unwrap();
}

/// Small helper to generate an SSE stream with a single assistant message.
fn sse_message(text: &str) -> String {
    const TEMPLATE: &str = r#"event: response.output_item.done
data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"TEXT_PLACEHOLDER"}]}}

event: response.completed
data: {"type":"response.completed","response":{"id":"resp1","output":[]}}


"#;

    TEMPLATE.replace("TEXT_PLACEHOLDER", text)
}

/// Helper to craft an SSE stream that returns a `function_call`.
fn sse_function_call() -> String {
    let call = serde_json::json!({
        "type": "response.output_item.done",
        "item": {
            "type": "function_call",
            "name": "shell",
            "arguments": "{\"command\":[\"echo\",\"hi\"]}",
            "call_id": "call1"
        }
    });
    let completed = serde_json::json!({
        "type": "response.completed",
        "response": {"id": "resp1", "output": []}
    });

    format!(
        "event: response.output_item.done\ndata: {call}\n\n\
event: response.completed\ndata: {completed}\n\n\n"
    )
}

/// SSE stream for the assistant's final message after the tool call returns.
fn sse_final_after_call() -> String {
    let msg = serde_json::json!({
        "type": "response.output_item.done",
        "item": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}]}
    });
    let completed = serde_json::json!({
        "type": "response.completed",
        "response": {"id": "resp2", "output": []}
    });

    format!(
        "event: response.output_item.done\ndata: {msg}\n\n\
event: response.completed\ndata: {completed}\n\n\n"
    )
}
