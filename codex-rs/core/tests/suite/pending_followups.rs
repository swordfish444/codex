use std::time::Duration;

use codex_core::ModelProviderInfo;
use codex_core::WireApi;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use core_test_support::load_sse_fixture_with_id;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event_with_timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

struct LastMessageMatcher {
    expected: String,
}

impl wiremock::Match for LastMessageMatcher {
    fn matches(&self, request: &wiremock::Request) -> bool {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
            return false;
        };
        let Some(input) = value.get("input").and_then(|input| input.as_array()) else {
            return false;
        };
        let Some(last) = input.last() else {
            return false;
        };
        let Some(content) = last.get("content").and_then(|content| content.as_array()) else {
            return false;
        };
        let Some(first_item) = content.first() else {
            return false;
        };
        let Some(text) = first_item.get("text").and_then(|text| text.as_str()) else {
            return false;
        };
        text == self.expected
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_follow_up_prompts_run_sequentially() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    let prompts = [
        ("first prompt", "resp-first"),
        ("follow-up one", "resp-second"),
        ("follow-up two", "resp-third"),
    ];

    for (text, response_id) in prompts {
        let sse = load_sse_fixture_with_id("tests/fixtures/completed_template.json", response_id);
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .and(LastMessageMatcher {
                expected: text.to_string(),
            })
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(sse, "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;
    }

    let provider = ModelProviderInfo {
        name: "mock-openai".into(),
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: Some("PATH".into()),
        env_key_instructions: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(1),
        stream_max_retries: Some(1),
        stream_idle_timeout_ms: Some(2_000),
        requires_openai_auth: false,
    };

    let test_codex = test_codex()
        .with_config(move |config| {
            config.base_instructions = Some("You are a helpful assistant.".to_string());
            config.model_provider = provider.clone();
        })
        .build(&server)
        .await
        .unwrap();

    let codex = test_codex.codex;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "first prompt".into(),
            }],
        })
        .await
        .unwrap();

    // Wait for the first task start before queuing follow-up prompts so the
    // initial turn is still in progress.
    wait_for_event_with_timeout(
        &codex,
        |ev| matches!(ev, EventMsg::TaskStarted(_)),
        Duration::from_secs(10),
    )
    .await;

    // Queue two follow-up prompts while the first turn completes. Prior to the
    // regression fix only the first follow-up would execute, leaving the second
    // one stuck indefinitely.
    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "follow-up one".into(),
            }],
        })
        .await
        .unwrap();
    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "follow-up two".into(),
            }],
        })
        .await
        .unwrap();

    wait_for_event_with_timeout(
        &codex,
        |ev| matches!(ev, EventMsg::TaskComplete(_)),
        Duration::from_secs(60),
    )
    .await;

    server.verify().await;
}
