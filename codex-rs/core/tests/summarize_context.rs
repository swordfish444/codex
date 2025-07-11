#![expect(clippy::unwrap_used, clippy::expect_used)]

//! Tests for the `Op::SummarizeContext` operation added to verify that
//! summarization requests are properly handled and injected as user input.

use std::time::Duration;

use codex_core::Codex;
use codex_core::ModelProviderInfo;
use codex_core::exec::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
use codex_core::protocol::AgentMessageEvent;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
mod test_support;
use tempfile::TempDir;
use test_support::load_default_config_for_test;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

/// Helper function to set up a codex session and wait for it to be configured
async fn setup_configured_codex_session() -> Codex {
    let codex_home = TempDir::new().unwrap();
    let config = load_default_config_for_test(&codex_home);
    let (codex, _, _) = codex_core::codex_wrapper::init_codex(config).await.unwrap();
    codex
}

/// Build SSE response with a message but WITHOUT completed marker (keeps task running)
fn sse_message_no_complete(message: &str) -> String {
    format!(
        "event: response.output_item.done\n\
data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"{message}\"}}]}}}}\n\n"
    )
}

/// Build SSE response with a message AND completed marker
fn sse_message_with_complete(id: &str, message: &str) -> String {
    format!(
        "event: response.output_item.done\n\
data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"{message}\"}}]}}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{id}\",\"output\":[]}}}}\n\n"
    )
}

#[tokio::test]
async fn test_summarize_context_spawns_new_agent_task() {
    // Test the specific behavior: when there's no current task,
    // SummarizeContext should spawn a new AgentTask with the summarization prompt
    let codex = setup_configured_codex_session().await;

    // At this point, there should be no current task running
    let _sub_id = codex.submit(Op::SummarizeContext).await.unwrap();

    // Should receive a TaskStarted event indicating a new AgentTask was spawned
    let event = timeout(Duration::from_secs(5), codex.next_event())
        .await
        .expect("timeout waiting for task started event")
        .expect("codex closed");

    assert!(
        matches!(event.msg, EventMsg::TaskStarted),
        "Expected TaskStarted when no current task exists - should spawn new AgentTask"
    );
}

#[tokio::test]
async fn test_summarize_context_injects_into_running_task() {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    // Set up mock server
    let server = MockServer::start().await;

    // Custom responder that tracks request count and responds differently
    struct SequentialResponder;
    impl Respond for SequentialResponder {
        fn respond(&self, req: &Request) -> ResponseTemplate {
            use std::sync::atomic::AtomicUsize;
            use std::sync::atomic::Ordering;
            static CALLS: AtomicUsize = AtomicUsize::new(0);
            let n = CALLS.fetch_add(1, Ordering::SeqCst);

            println!(
                "Mock server received request #{n}: {}",
                std::str::from_utf8(&req.body).unwrap_or("invalid utf8")
            );

            if n == 0 {
                // First request: respond to initial message but DON'T complete the task
                let response = sse_message_no_complete(
                    "I understand you need help with a coding task. Please go ahead and explain what you're trying to do.",
                );
                println!("Sending response without complete: {}", response);
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(response, "text/event-stream")
            } else {
                // Second request: this should be the summary request
                let response = sse_message_with_complete(
                    "resp2",
                    "Here's a summary of our conversation: You mentioned needing help with a coding task and were about to explain what you're trying to do.",
                );
                println!("Sending response with complete: {}", response);
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(response, "text/event-stream")
            }
        }
    }

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(SequentialResponder {})
        .expect(2)
        .mount(&server)
        .await;

    // Configure environment
    unsafe {
        std::env::set_var("OPENAI_REQUEST_MAX_RETRIES", "0");
        std::env::set_var("OPENAI_STREAM_MAX_RETRIES", "0");
    }

    let model_provider = ModelProviderInfo {
        name: "openai".into(),
        base_url: format!("{}/v1", server.uri()),
        env_key: Some("PATH".into()),
        env_key_instructions: None,
        wire_api: codex_core::WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
    };

    // Set up codex with mock configuration
    let codex_home = TempDir::new().unwrap();
    let mut config = load_default_config_for_test(&codex_home);
    config.model_provider = model_provider;
    let ctrl_c = std::sync::Arc::new(tokio::sync::Notify::new());
    let (codex, _) = Codex::spawn(config, ctrl_c).await.unwrap();

    // Wait for SessionConfigured event
    let event = timeout(Duration::from_secs(5), codex.next_event())
        .await
        .expect("timeout waiting for session configured")
        .expect("codex closed");

    assert!(
        matches!(event.msg, EventMsg::SessionConfigured(_)),
        "Expected SessionConfigured event, got: {:?}",
        event.msg
    );

    // First, start a task by submitting user input
    let _input_sub_id = codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text:
                    "I need help with a coding task. First, let me explain what I'm trying to do..."
                        .to_string(),
            }],
        })
        .await
        .unwrap();

    // Wait for the task to start
    let event = timeout(Duration::from_secs(5), codex.next_event())
        .await
        .expect("timeout waiting for task started")
        .expect("codex closed");

    println!("Got event after UserInput: {:?}", event.msg);

    assert!(
        matches!(event.msg, EventMsg::TaskStarted),
        "First task should start"
    );

    // Wait for the initial response message
    let mut got_initial_response = false;
    while !got_initial_response {
        let event = timeout(Duration::from_secs(5), codex.next_event())
            .await
            .expect("timeout waiting for initial response")
            .expect("codex closed");

        match event.msg {
            EventMsg::AgentReasoning(_) | EventMsg::TokenCount(_) => continue,
            EventMsg::AgentMessage(_) => {
                got_initial_response = true;
            }
            EventMsg::TaskComplete(_) => {
                panic!(
                    "Task should NOT complete after first message - mock should keep it running"
                );
            }
            other => panic!("Unexpected event: {other:?}"),
        }
    }

    // Now submit SummarizeContext while the task is still running
    let _summary_sub_id = codex.submit(Op::SummarizeContext).await.unwrap();

    // We should NOT get a new TaskStarted event (that would mean a new task was spawned)
    // Instead we should get an AgentMessage with the summary
    loop {
        let event = timeout(Duration::from_secs(5), codex.next_event())
            .await
            .expect("timeout waiting for summary message")
            .expect("codex closed");

        match event.msg {
            EventMsg::TaskStarted => {
                panic!(
                    "Got TaskStarted - summary request spawned a new task instead of injecting into existing one!"
                );
            }
            EventMsg::AgentReasoning(_) | EventMsg::TokenCount(_) => continue,
            EventMsg::AgentMessage(AgentMessageEvent { message }) => {
                // Verify this is the summary message
                assert!(
                    message.contains("summary") || message.contains("conversation"),
                    "Expected summary content, got: {message}"
                );
                break;
            }
            EventMsg::TaskComplete(_) => {
                // This is OK after we get the summary message
                break;
            }
            other => panic!("Unexpected event: {other:?}"),
        }
    }
}
