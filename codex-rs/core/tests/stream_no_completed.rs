//! Verifies that the agent retries when the SSE stream terminates before
//! delivering a `response.completed` event.

use std::time::Duration;

use codex_core::Codex;
use codex_core::ModelProviderInfo;
use codex_core::exec::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
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

/// Load the SSE event sequences for the test from a JSON fixture.
///
/// The fixture lives in `tests/fixtures/stream_retry.json` and defines two
/// arrays of events: `first` and `second`. Each entry represents the JSON body
/// of a single SSE event. When the Responses API evolves with new fields or
/// event kinds simply update that JSON file or add a new one and reference it
/// here.
fn load_fixture() -> (String, String) {
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;

    fn events_to_sse(events: &[Value]) -> String {
        let mut out = String::new();
        for ev in events {
            let kind = ev
                .get("type")
                .and_then(Value::as_str)
                .expect("event missing type");
            out.push_str("event: ");
            out.push_str(kind);
            out.push('\n');
            out.push_str("data: ");
            out.push_str(&ev.to_string());
            out.push_str("\n\n");
        }
        out
    }

    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "stream_retry.json",
    ]
    .iter()
    .collect();
    let raw = fs::read_to_string(path).expect("fixture missing");
    let v: Value = serde_json::from_str(&raw).expect("invalid fixture JSON");
    let first = events_to_sse(v.get("first").and_then(Value::as_array).unwrap());
    let second = events_to_sse(v.get("second").and_then(Value::as_array).unwrap());
    (first, second)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retries_on_early_close() {
    #![allow(clippy::unwrap_used)]

    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    let server = MockServer::start().await;
    // Convert the JSON fixture into SSE event strings for the two mock calls.
    let (sse_first, sse_second) = load_fixture();

    struct SeqResponder {
        first: String,
        second: String,
    }
    impl Respond for SeqResponder {
        fn respond(&self, _: &Request) -> ResponseTemplate {
            use std::sync::atomic::AtomicUsize;
            use std::sync::atomic::Ordering;
            static CALLS: AtomicUsize = AtomicUsize::new(0);
            let n = CALLS.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(self.first.clone(), "text/event-stream")
            } else {
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(self.second.clone(), "text/event-stream")
            }
        }
    }

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(SeqResponder {
            first: sse_first,
            second: sse_second,
        })
        .expect(2)
        .mount(&server)
        .await;

    // Environment
    //
    // As of Rust 2024 `std::env::set_var` has been made `unsafe` because
    // mutating the process environment is inherently racy when other threads
    // are running.  We therefore have to wrap every call in an explicit
    // `unsafe` block.  These are limited to the test-setup section so the
    // scope is very small and clearly delineated.

    unsafe {
        std::env::set_var("OPENAI_REQUEST_MAX_RETRIES", "0");
        std::env::set_var("OPENAI_STREAM_MAX_RETRIES", "1");
        std::env::set_var("OPENAI_STREAM_IDLE_TIMEOUT_MS", "2000");
    }

    let model_provider = ModelProviderInfo {
        name: "openai".into(),
        base_url: format!("{}/v1", server.uri()),
        // Environment variable that should exist in the test environment.
        // ModelClient will return an error if the environment variable for the
        // provider is not set.
        env_key: Some("PATH".into()),
        env_key_instructions: None,
        wire_api: codex_core::WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
    };

    let ctrl_c = std::sync::Arc::new(tokio::sync::Notify::new());
    let codex_home = TempDir::new().unwrap();
    let mut config = load_default_config_for_test(&codex_home);
    config.model_provider = model_provider;
    let (codex, _init_id) = Codex::spawn(config, ctrl_c).await.unwrap();

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "hello".into(),
            }],
        })
        .await
        .unwrap();

    // Wait until TaskComplete (should succeed after retry).
    loop {
        let ev = timeout(Duration::from_secs(10), codex.next_event())
            .await
            .unwrap()
            .unwrap();
        if matches!(ev.msg, EventMsg::TaskComplete(_)) {
            break;
        }
    }
}
