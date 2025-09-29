use codex_protocol::protocol::AgentMessageDeltaEvent;
use codex_protocol::protocol::AgentReasoningDeltaEvent;
use codex_protocol::protocol::AgentReasoningSectionBreakEvent;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::InputItem;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::Submission;
use codex_protocol::protocol::TaskStartedEvent;
use codex_protocol::protocol::TokenCountEvent;
use codex_protocol::protocol::TokenUsageInfo;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::test_codex::test_codex;
use serde_json::json;

fn build_user_input(text: &str) -> Op {
    Op::UserInput {
        items: vec![InputItem::Text {
            text: text.to_string(),
        }],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn submit_returns_id_and_events_share_it() {
    core_test_support::skip_if_no_network!();

    let server = start_mock_server().await;
    let body = sse(vec![
        ev_assistant_message("resp-item", "First turn complete."),
        ev_completed("resp-complete"),
    ]);
    mount_sse_sequence(&server, vec![body]).await;

    let mut builder = test_codex();
    let codex = builder.build(&server).await.unwrap().codex;

    let submission_id = codex
        .submit(build_user_input("first turn"))
        .await
        .expect("submit succeeds");
    assert!(!submission_id.is_empty(), "submit should return id");

    let mut saw_agent_message = false;
    loop {
        let event = codex.next_event().await.expect("event available");
        assert_eq!(event.id, submission_id, "event id should match submission");

        match event.msg {
            EventMsg::AgentMessage(ev) => {
                saw_agent_message = true;
                assert_eq!(ev.message, "First turn complete.");
            }
            EventMsg::TaskComplete(ev) => {
                assert_eq!(
                    ev.last_agent_message.as_deref(),
                    Some("First turn complete."),
                );
                break;
            }
            _ => {}
        }
    }

    assert!(saw_agent_message, "expected AgentMessage event");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn submit_with_id_uses_caller_supplied_id() {
    core_test_support::skip_if_no_network!();

    let server = start_mock_server().await;
    let body = sse(vec![
        ev_assistant_message("resp-item", "Acknowledged."),
        ev_completed("resp-complete"),
    ]);
    mount_sse_sequence(&server, vec![body]).await;

    let mut builder = test_codex();
    let codex = builder.build(&server).await.unwrap().codex;

    let custom_id = "custom-submission-id".to_string();
    codex
        .submit_with_id(Submission {
            id: custom_id.clone(),
            op: build_user_input("please acknowledge"),
        })
        .await
        .expect("submit_with_id succeeds");

    loop {
        let event = codex.next_event().await.expect("event available");
        assert_eq!(event.id, custom_id, "event id should match provided id");

        if let EventMsg::TaskComplete(ev) = event.msg {
            assert_eq!(ev.last_agent_message.as_deref(), Some("Acknowledged."));
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn submit_streams_reasoning_and_token_usage_events() {
    core_test_support::skip_if_no_network!();

    let server = start_mock_server().await;
    let response_id = "resp-stream";
    let body = sse(vec![
        json!({
            "type": "response.created",
            "response": {"id": response_id}
        }),
        json!({
            "type": "response.output_text.delta",
            "delta": "Partial "
        }),
        json!({
            "type": "response.reasoning_summary_text.delta",
            "delta": "Drafting plan"
        }),
        json!({
            "type": "response.reasoning_summary_part.added"
        }),
        ev_assistant_message("resp-item", "Partial Final output."),
        json!({
            "type": "response.completed",
            "response": {
                "id": response_id,
                "usage": {
                    "input_tokens": 40,
                    "input_tokens_details": {"cached_tokens": 5},
                    "output_tokens": 12,
                    "output_tokens_details": {"reasoning_tokens": 3},
                    "total_tokens": 52
                }
            }
        }),
    ]);
    mount_sse_sequence(&server, vec![body]).await;

    let mut builder = test_codex();
    let codex = builder.build(&server).await.unwrap().codex;

    let submission_id = codex
        .submit(build_user_input("stream detailed output"))
        .await
        .expect("submit succeeds");

    let mut saw_text_delta = false;
    let mut saw_reasoning_delta = false;
    let mut saw_agent_message = false;
    let mut final_message: Option<String> = None;
    let mut token_count: Option<TokenCountEvent> = None;

    loop {
        let event = codex.next_event().await.expect("event available");
        assert_eq!(event.id, submission_id);

        match event.msg {
            EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
                saw_text_delta = true;
                assert_eq!(delta, "Partial ");
            }
            EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta }) => {
                saw_reasoning_delta = true;
                assert_eq!(delta, "Drafting plan");
            }
            EventMsg::AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent {}) => {}
            EventMsg::AgentMessage(ev) => {
                saw_agent_message = true;
                final_message = Some(ev.message);
            }
            EventMsg::TokenCount(ev) => {
                token_count = Some(ev);
            }
            EventMsg::TaskComplete(ev) => {
                assert_eq!(
                    ev.last_agent_message.as_deref(),
                    Some("Partial Final output."),
                );
                break;
            }
            _ => {}
        }
    }

    assert!(saw_text_delta, "expected streaming text delta event");
    assert!(saw_reasoning_delta, "expected reasoning summary delta");
    // Some model responses may omit explicit reasoning section boundaries even when
    // reasoning deltas stream, so treat the section break as optional.
    assert!(saw_agent_message, "expected agent message event");
    assert_eq!(final_message.as_deref(), Some("Partial Final output."));

    let token_count = token_count.expect("expected token count event");
    let info: &TokenUsageInfo = token_count
        .info
        .as_ref()
        .expect("token usage info should be present");
    assert_eq!(info.last_token_usage.total_tokens, 52);
    assert_eq!(info.last_token_usage.input_tokens, 40);
    assert_eq!(info.last_token_usage.output_tokens, 12);
    assert_eq!(info.last_token_usage.reasoning_output_tokens, 3);
    assert_eq!(info.last_token_usage.cached_input_tokens, 5);
    assert_eq!(info.total_token_usage.total_tokens, 52);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_submissions_emit_distinct_ids_and_token_totals() {
    core_test_support::skip_if_no_network!();

    let server = start_mock_server().await;
    let first_body = sse(vec![
        json!({"type": "response.created", "response": {"id": "resp-first"}}),
        ev_assistant_message("resp-first-item", "Turn one done."),
        json!({
            "type": "response.completed",
            "response": {
                "id": "resp-first",
                "usage": {
                    "input_tokens": 10,
                    "input_tokens_details": {"cached_tokens": 0},
                    "output_tokens": 4,
                    "output_tokens_details": {"reasoning_tokens": 1},
                    "total_tokens": 14
                }
            }
        }),
    ]);
    let second_body = sse(vec![
        json!({"type": "response.created", "response": {"id": "resp-second"}}),
        json!({
            "type": "response.output_text.delta",
            "delta": "Streaming second "
        }),
        ev_assistant_message("resp-second-item", "Turn two complete."),
        json!({
            "type": "response.completed",
            "response": {
                "id": "resp-second",
                "usage": {
                    "input_tokens": 12,
                    "input_tokens_details": {"cached_tokens": 2},
                    "output_tokens": 6,
                    "output_tokens_details": {"reasoning_tokens": 2},
                    "total_tokens": 20
                }
            }
        }),
    ]);
    mount_sse_sequence(&server, vec![first_body, second_body]).await;

    let mut builder = test_codex();
    let codex = builder.build(&server).await.unwrap().codex;

    let first_id = codex
        .submit(build_user_input("first turn"))
        .await
        .expect("first submit succeeds");
    assert!(
        !first_id.is_empty(),
        "first submission id should be present"
    );

    let mut saw_first_task_started = false;
    let mut saw_first_token_count: Option<TokenCountEvent> = None;
    let mut first_completed = false;

    while !first_completed {
        let event = codex.next_event().await.expect("first turn event");
        assert_eq!(event.id, first_id);

        match event.msg {
            EventMsg::TaskStarted(TaskStartedEvent { .. }) => {
                saw_first_task_started = true;
            }
            EventMsg::AgentMessage(message) => {
                assert_eq!(message.message, "Turn one done.");
            }
            EventMsg::TokenCount(ev) => {
                saw_first_token_count = Some(ev);
            }
            EventMsg::TaskComplete(_) => {
                first_completed = true;
            }
            _ => {}
        }
    }

    assert!(saw_first_task_started, "first turn should emit TaskStarted");
    let first_token = saw_first_token_count.expect("first turn should emit TokenCount");
    let first_usage: &TokenUsageInfo = first_token
        .info
        .as_ref()
        .expect("usage info should exist for first turn");
    assert_eq!(first_usage.last_token_usage.total_tokens, 14);

    let custom_id = "manual-second".to_string();
    codex
        .submit_with_id(Submission {
            id: custom_id.clone(),
            op: build_user_input("second turn"),
        })
        .await
        .expect("second submit succeeds");

    let mut saw_second_task_started = false;
    let mut saw_second_token_count: Option<TokenCountEvent> = None;

    loop {
        let event = codex.next_event().await.expect("second turn event");
        assert_eq!(event.id, custom_id);

        match event.msg {
            EventMsg::TaskStarted(TaskStartedEvent { .. }) => {
                saw_second_task_started = true;
            }
            EventMsg::AgentMessage(message) => {
                assert_eq!(message.message, "Turn two complete.");
            }
            EventMsg::TokenCount(ev) => {
                saw_second_token_count = Some(ev);
            }
            EventMsg::TaskComplete(ev) => {
                assert_eq!(ev.last_agent_message.as_deref(), Some("Turn two complete."),);
                break;
            }
            _ => {}
        }
    }

    assert!(
        saw_second_task_started,
        "second turn should emit TaskStarted"
    );
    let second_token = saw_second_token_count.expect("second turn should emit TokenCount");
    let second_usage: &TokenUsageInfo = second_token
        .info
        .as_ref()
        .expect("usage info should exist for second turn");
    assert_eq!(second_usage.last_token_usage.total_tokens, 20);
    assert_eq!(second_usage.total_token_usage.total_tokens, 34);
    assert!(
        second_usage.total_token_usage.total_tokens > first_usage.total_token_usage.total_tokens
    );
}
