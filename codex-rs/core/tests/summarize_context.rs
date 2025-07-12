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

    let event = timeout(Duration::from_secs(5), codex.next_event())
        .await
        .expect("timeout waiting for task started event")
        .expect("codex closed");

    assert!(
        matches!(event.msg, EventMsg::TaskStarted),
        "Expected TaskStarted when no current task exists - should spawn new AgentTask"
    );
}
