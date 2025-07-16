#![expect(clippy::unwrap_used)]

use assert_cmd::Command as AssertCommand;
use codex_core::exec::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
use tempfile::TempDir;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

/// Tests streaming chat completions through the CLI using a mock server.
/// This test:
/// 1. Sets up a mock server that simulates OpenAI's chat completions API
/// 2. Configures codex to use this mock server via a custom provider
/// 3. Sends a simple "hello?" prompt and verifies the streamed response
/// 4. Ensures the response is received exactly once and contains "hi"
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_mode_stream_cli() {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    let server = MockServer::start().await;
    let sse = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{}}]}\n\n",
        "data: [DONE]\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(sse, "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let home = TempDir::new().unwrap();
    let provider_override = format!(
        "model_providers.mock={{ name = \"mock\", base_url = \"{}/v1\", env_key = \"PATH\", wire_api = \"chat\" }}",
        server.uri()
    );
    let mut cmd = AssertCommand::new("cargo");
    cmd.arg("run")
        .arg("-p")
        .arg("codex-cli")
        .arg("--quiet")
        .arg("--")
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("-c")
        .arg(&provider_override)
        .arg("-c")
        .arg("model_provider=\"mock\"")
        .arg("-c")
        .arg("streaming=false")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("hello?");
    cmd.env("CODEX_HOME", home.path())
        .env("OPENAI_API_KEY", "dummy")
        .env("OPENAI_BASE_URL", format!("{}/v1", server.uri()));

    let output = cmd.output().unwrap();
    println!("Status: {}", output.status);
    println!("Stdout:\n{}", String::from_utf8_lossy(&output.stdout));
    println!("Stderr:\n{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let hi_lines = stdout.lines().filter(|line| line.trim() == "hi").count();
    assert_eq!(hi_lines, 1, "Expected exactly one line with 'hi'");

    server.verify().await;
}

/// Tests streaming responses through the CLI using a local SSE fixture file.
/// This test:
/// 1. Uses a pre-recorded SSE response fixture instead of a live server
/// 2. Configures codex to read from this fixture via CODEX_RS_SSE_FIXTURE env var
/// 3. Sends a "hello?" prompt and verifies the response
/// 4. Ensures the fixture content is correctly streamed through the CLI
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_api_stream_cli() {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cli_responses_fixture.sse");

    let home = TempDir::new().unwrap();
    let mut cmd = AssertCommand::new("cargo");
    cmd.arg("run")
        .arg("-p")
        .arg("codex-cli")
        .arg("--quiet")
        .arg("--")
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("-c")
        .arg("streaming=false")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("hello?");
    cmd.env("CODEX_HOME", home.path())
        .env("OPENAI_API_KEY", "dummy")
        .env("CODEX_RS_SSE_FIXTURE", fixture)
        .env("OPENAI_BASE_URL", "http://unused.local");

    let output = cmd.output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fixture hello"));
}

/// Tests chat completions with streaming enabled (streaming=true) through the CLI using a mock server.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_mode_streaming_enabled_cli() {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    let server = MockServer::start().await;
    // Simulate streaming deltas: 'h' and 'i' as separate chunks
    let sse = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"h\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"i\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{}}]}\n\n",
        "data: [DONE]\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(sse, "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let home = TempDir::new().unwrap();
    let provider_override = format!(
        "model_providers.mock={{ name = \"mock\", base_url = \"{}/v1\", env_key = \"PATH\", wire_api = \"chat\" }}",
        server.uri()
    );
    let mut cmd = AssertCommand::new("cargo");
    cmd.arg("run")
        .arg("-p")
        .arg("codex-cli")
        .arg("--quiet")
        .arg("--")
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("-c")
        .arg(&provider_override)
        .arg("-c")
        .arg("model_provider=\"mock\"")
        .arg("-c")
        .arg("streaming=true")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("hello?");
    cmd.env("CODEX_HOME", home.path())
        .env("OPENAI_API_KEY", "dummy")
        .env("OPENAI_BASE_URL", format!("{}/v1", server.uri()));

    let output = cmd.output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Assert that 'h' and 'i' are output as two separate chunks from stdout, not as a single chunk
    // We split the output on 'h' and 'i' and check their order and separation
    let mut chunks = Vec::new();
    let mut last = 0;
    for (idx, c) in stdout.char_indices() {
        if c == 'h' || c == 'i' {
            if last != idx {
                let chunk = &stdout[last..idx];
                if !chunk.trim().is_empty() {
                    chunks.push(chunk);
                }
            }
            chunks.push(&stdout[idx..idx + c.len_utf8()]);
            last = idx + c.len_utf8();
        }
    }
    if last < stdout.len() {
        let chunk = &stdout[last..];
        if !chunk.trim().is_empty() {
            chunks.push(chunk);
        }
    }
    // Only keep the 'h' and 'i' chunks
    let delta_chunks: Vec<&str> = chunks
        .iter()
        .cloned()
        .filter(|s| *s == "h" || *s == "i")
        .collect();
    assert_eq!(
        delta_chunks,
        vec!["h", "i"],
        "Expected two separate delta chunks 'h' and 'i' from stdout"
    );

    server.verify().await;
}

/// Tests responses API with streaming enabled (streaming=true) through the CLI using a local SSE fixture file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responses_api_streaming_enabled_cli() {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    // Create a fixture with two deltas: 'fixture ' and 'hello'
    use std::fs;
    use std::io::Write;
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/cli_responses_fixture_streaming.sse");
    let mut fixture_file = fs::File::create(&fixture_path).unwrap();
    writeln!(fixture_file, "event: response.created").unwrap();
    writeln!(
        fixture_file,
        "data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"resp1\"}}}}\n"
    )
    .unwrap();
    writeln!(fixture_file, "event: response.output_text.delta").unwrap();
    writeln!(fixture_file, "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"fixture \",\"item_id\":\"msg1\"}}\n").unwrap();
    writeln!(fixture_file, "event: response.output_text.delta").unwrap();
    writeln!(fixture_file, "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"hello\",\"item_id\":\"msg1\"}}\n").unwrap();
    writeln!(fixture_file, "event: response.output_text.done").unwrap();
    writeln!(fixture_file, "data: {{\"type\":\"response.output_text.done\",\"text\":\"fixture hello\",\"item_id\":\"msg1\"}}\n").unwrap();
    writeln!(fixture_file, "event: response.output_item.done").unwrap();
    writeln!(fixture_file, "data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"fixture hello\"}}]}}}}\n").unwrap();
    writeln!(fixture_file, "event: response.completed").unwrap();
    writeln!(fixture_file, "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp1\",\"output\":[]}}}}\n").unwrap();

    let home = TempDir::new().unwrap();
    let mut cmd = AssertCommand::new("cargo");
    cmd.arg("run")
        .arg("-p")
        .arg("codex-cli")
        .arg("--quiet")
        .arg("--")
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("-c")
        .arg("streaming=true")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("hello?");
    cmd.env("CODEX_HOME", home.path())
        .env("OPENAI_API_KEY", "dummy")
        .env("CODEX_RS_SSE_FIXTURE", &fixture_path)
        .env("OPENAI_BASE_URL", "http://unused.local");

    let output = cmd.output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Assert that 'fixture ' and 'hello' are output as two separate chunks from stdout, not as a single chunk
    // We split the output on the known delta substrings and check their order and separation
    let mut chunks = Vec::new();
    let mut last = 0;
    for pat in ["fixture ", "hello"] {
        if let Some(idx) = stdout[last..].find(pat) {
            if last != last + idx {
                let chunk = &stdout[last..last + idx];
                if !chunk.trim().is_empty() {
                    chunks.push(chunk);
                }
            }
            chunks.push(&stdout[last + idx..last + idx + pat.len()]);
            last = last + idx + pat.len();
        }
    }
    if last < stdout.len() {
        let chunk = &stdout[last..];
        if !chunk.trim().is_empty() {
            chunks.push(chunk);
        }
    }
    // Only keep the delta chunks
    let delta_chunks: Vec<&str> = chunks
        .iter()
        .cloned()
        .filter(|s| *s == "fixture " || *s == "hello")
        .collect();
    assert_eq!(
        delta_chunks,
        vec!["fixture ", "hello"],
        "Expected two separate delta chunks 'fixture ' and 'hello' from stdout"
    );
}
