use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use codex_core::protocol::ReviewRequest;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;

/// Smoke test: ensure regular, review, and compact tasks can run sequentially in
/// a single session without leaving dangling state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn regular_review_compact_sequence() {
    skip_if_no_network!();

    let server = start_mock_server().await;

    let regular_body = sse(vec![
        ev_assistant_message("regular-msg", "First turn complete."),
        ev_completed("regular-resp"),
    ]);

    let review_json = serde_json::json!({
        "findings": [],
        "overall_correctness": "ok",
        "overall_explanation": "Looks good overall.",
        "overall_confidence_score": 0.5
    })
    .to_string();
    let review_body = sse(vec![
        ev_assistant_message("review-msg", &review_json),
        ev_completed("review-resp"),
    ]);

    let compact_body = sse(vec![
        ev_assistant_message("compact-msg", "Session summary."),
        ev_completed("compact-resp"),
    ]);

    mount_sse_sequence(&server, vec![regular_body, review_body, compact_body]).await;

    let codex = test_codex().build(&server).await.unwrap().codex;

    // 1. Regular user input turn.
    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "first turn".into(),
            }],
        })
        .await
        .unwrap();
    let regular_complete =
        wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;
    let regular_last = match regular_complete {
        EventMsg::TaskComplete(ev) => ev.last_agent_message,
        other => panic!("expected TaskComplete for regular turn, got {other:?}"),
    };
    assert_eq!(regular_last.as_deref(), Some("First turn complete."));

    // 2. Review task turn.
    codex
        .submit(Op::Review {
            review_request: ReviewRequest {
                prompt: "please review".to_string(),
                user_facing_hint: "hint".to_string(),
            },
        })
        .await
        .unwrap();

    let _entered = wait_for_event(&codex, |ev| matches!(ev, EventMsg::EnteredReviewMode(_))).await;
    let exited = wait_for_event(&codex, |ev| matches!(ev, EventMsg::ExitedReviewMode(_))).await;
    let review_output = match exited {
        EventMsg::ExitedReviewMode(ev) => ev.review_output.expect("expected review output"),
        other => panic!("expected ExitedReviewMode, got {other:?}"),
    };
    assert_eq!(review_output.overall_correctness, "ok");
    assert_eq!(review_output.overall_explanation, "Looks good overall.");
    assert!(review_output.findings.is_empty());

    let review_complete =
        wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;
    let review_last = match review_complete {
        EventMsg::TaskComplete(ev) => ev.last_agent_message,
        other => panic!("expected TaskComplete for review, got {other:?}"),
    }
    .expect("review task should emit last_agent_message");
    assert_eq!(review_last, review_json);

    // 3. Manual compact task.
    codex.submit(Op::Compact).await.unwrap();
    let compact_complete =
        wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;
    let compact_last = match compact_complete {
        EventMsg::TaskComplete(ev) => ev.last_agent_message,
        other => panic!("expected TaskComplete for compact, got {other:?}"),
    };
    assert!(
        compact_last.is_none(),
        "compact task should not emit a trailing assistant message"
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        3,
        "expected exactly three Responses API calls"
    );
}
