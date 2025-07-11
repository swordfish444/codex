#![expect(clippy::unwrap_used, clippy::expect_used)]

//! Tests for the `Op::SummarizeContext` operation added to verify that
//! summarization requests are properly handled and injected as user input.

use std::time::Duration;

use codex_core::Codex;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
mod test_support;
use tempfile::TempDir;
use test_support::load_default_config_for_test;
use tokio::sync::Notify;
use tokio::time::timeout;

/// Helper function to set up a codex session and wait for it to be configured
async fn setup_configured_codex_session() -> Codex {
    let codex_home = TempDir::new().unwrap();
    let config = load_default_config_for_test(&codex_home);
    let (codex, _init_id) = Codex::spawn(config, std::sync::Arc::new(Notify::new()))
        .await
        .unwrap();

    // Wait for session configured
    loop {
        let event = timeout(Duration::from_secs(5), codex.next_event())
            .await
            .expect("timeout waiting for session configured")
            .expect("codex closed");

        if matches!(event.msg, EventMsg::SessionConfigured(_)) {
            break;
        }
    }

    codex
}

#[tokio::test]
async fn test_summarize_context_spawns_new_agent_task() {
    // Test the specific behavior: when there's no current task,
    // SummarizeContext should spawn a new AgentTask with the summarization prompt
    let codex = setup_configured_codex_session().await;

    // At this point, there should be no current task running
    // Submit SummarizeContext operation - this should trigger:
    // if let Err(items) = sess.inject_input(summarization_prompt) {
    //     let task = AgentTask::spawn(Arc::clone(sess), sub.id, items);
    //     sess.set_task(task);
    // }
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
    // Test that when a task IS running, SummarizeContext injects into the existing task
    let codex = setup_configured_codex_session().await;

    // First, start a task by submitting user input
    let _input_sub_id = codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "Hello, this should start a task".to_string(),
            }],
        })
        .await
        .unwrap();

    // Wait for the task to start
    let event = timeout(Duration::from_secs(5), codex.next_event())
        .await
        .expect("timeout waiting for task started")
        .expect("codex closed");

    assert!(
        matches!(event.msg, EventMsg::TaskStarted),
        "First task should start"
    );

    // Now submit SummarizeContext while a task is running
    // This should test the inject_input SUCCESS path (not the spawn new task path)
    let _summary_sub_id = codex.submit(Op::SummarizeContext).await.unwrap();

    // The summarization prompt should be injected into the existing task
    // rather than spawning a new one. We shouldn't get another TaskStarted event
    let result = timeout(Duration::from_millis(500), codex.next_event()).await;

    // If we get an event, it should NOT be TaskStarted (since we're injecting into existing task)
    if let Ok(Ok(event)) = result {
        assert!(
            !matches!(event.msg, EventMsg::TaskStarted),
            "Should not spawn new task when one is already running - should inject instead"
        );
    }
    // If we timeout, that's expected - no immediate event for successful injection
}
