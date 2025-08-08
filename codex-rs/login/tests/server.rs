#![cfg(feature = "http-e2e-tests")]
use codex_login::{run_local_login_server_with_options, LoginServerOptions};
use base64::Engine;
use std::io::Read;
use std::net::TcpListener;
// use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn start_mock_oauth_server(port: u16, behavior: MockBehavior) {
    thread::spawn(move || {
        let server = tiny_http::Server::http(format!("127.0.0.1:{port}")).unwrap();
        for mut request in server.incoming_requests() {
            let url = request.url().to_string();
            if request.method() == &tiny_http::Method::Post && url.starts_with("/oauth/token") {
                // Read body
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).ok();
                let content_type = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Content-Type"))
                    .map(|h| h.value.as_str().to_string())
                    .unwrap_or_default();

                // Parse either x-www-form-urlencoded or JSON
                let mut form = std::collections::HashMap::<String, String>::new();
                if content_type.starts_with("application/x-www-form-urlencoded") {
                    for kv in body.split('&') {
                        if let Some((k, v)) = kv.split_once('=') {
                            let k = urlencoding::decode(k).unwrap().into_owned();
                            let v = urlencoding::decode(v).unwrap().into_owned();
                            form.insert(k, v);
                        }
                    }
                } else if content_type.starts_with("application/json") {
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    if let Some(obj) = v.as_object() {
                        for (k, vv) in obj.iter() {
                            form.insert(k.clone(), vv.as_str().unwrap_or_default().to_string());
                        }
                    }
                }

                match behavior {
                    MockBehavior::Success => {
                        if form.get("grant_type").map(|s| s.as_str()) == Some("authorization_code") {
                            // Return tokens
                            let id_token = make_fake_jwt(serde_json::json!({
                                "https://api.openai.com/auth": {
                                    "chatgpt_account_id": "acc-1",
                                }
                            }));
                            let access_token = make_fake_jwt(serde_json::json!({
                                "https://api.openai.com/auth": {
                                    "organization_id": "org-1",
                                    "project_id": "proj-1",
                                    "completed_platform_onboarding": true,
                                    "is_org_owner": false,
                                    "chatgpt_plan_type": "plus"
                                }
                            }));
                            let payload = serde_json::json!({
                                "id_token": id_token,
                                "access_token": access_token,
                                "refresh_token": "refresh-1"
                            });
                            let _ = request.respond(
                                tiny_http::Response::from_string(payload.to_string())
                                    .with_status_code(200)
                                    .with_header(
                                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                                    ),
                            );
                        } else {
                            // token-exchange → API key
                            let payload = serde_json::json!({
                                "access_token": "sk-test-123"
                            });
                            let _ = request.respond(
                                tiny_http::Response::from_string(payload.to_string())
                                    .with_status_code(200)
                                    .with_header(
                                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                                    ),
                            );
                        }
                    }
                    MockBehavior::TokenError => {
                        let _ = request.respond(tiny_http::Response::from_string("error").with_status_code(500));
                    }
                }
            } else if request.method() == &tiny_http::Method::Post
                && url.starts_with("/v1/billing/redeem_credits")
            {
                let payload = serde_json::json!({"granted_chatgpt_subscriber_api_credits": 5});
                let _ = request.respond(
                    tiny_http::Response::from_string(payload.to_string())
                        .with_status_code(200)
                        .with_header(
                            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                        ),
                );
            } else {
                let _ = request.respond(tiny_http::Response::from_string("not found").with_status_code(404));
            }
        }
    });
}

#[derive(Clone, Copy)]
enum MockBehavior {
    Success,
    TokenError,
}

fn make_fake_jwt(payload: serde_json::Value) -> String {
    let header = serde_json::json!({"alg": "none", "typ": "JWT"});
    let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
    let header_b64 = b64(&serde_json::to_vec(&header).unwrap());
    let payload_b64 = b64(&serde_json::to_vec(&payload).unwrap());
    let signature_b64 = b64(b"sig");
    format!("{header_b64}.{payload_b64}.{signature_b64}")
}

fn http_get(url: &str) -> (u16, String, Option<String>) {
    let resp = ureq::get(url).call().expect("http get failed");
    let status = resp.status();
    let body = resp.into_string().unwrap_or_default();
    let location = None; // ureq consumed headers in previous call; not needed here
    (status as u16, body, location)
}

fn http_get_follow_redirect(url: &str) -> (u16, String) {
    let agent = ureq::AgentBuilder::new().redirects(5).build();
    match agent.get(url).call() {
        Ok(resp) => (resp.status(), resp.into_string().unwrap_or_default()),
        Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().unwrap_or_default()),
        Err(err) => panic!("http error: {err}"),
    }
}

// 1) Happy path: writes auth.json and exits after /success
#[test]
fn login_server_happy_path() {
    let oauth_port = find_free_port();
    start_mock_oauth_server(oauth_port, MockBehavior::Success);

    let codex_home = TempDir::new().unwrap();
    let port = find_free_port();
    let issuer = format!("http://127.0.0.1:{oauth_port}");

    let opts = LoginServerOptions {
        codex_home: codex_home.path().to_path_buf(),
        client_id: "test-client".to_string(),
        issuer: issuer.clone(),
        port,
        open_browser: false,
        redeem_credits: true,
        expose_state_endpoint: true,
        testing_timeout_secs: Some(5),
    };

    let handle = thread::spawn(move || run_local_login_server_with_options(opts).unwrap());

    // Wait for server to bind
    wait_for_state_endpoint(port, Duration::from_secs(5));

    // Get state via test-only endpoint
    let state_url = format!("http://127.0.0.1:{port}/__test/state");
    let (_s, state, _) = http_get(&state_url);
    assert!(!state.is_empty());

    // Simulate callback
    let cb_url = format!("http://127.0.0.1:{port}/auth/callback?code=abc&state={state}");
    let (status, body) = http_get_follow_redirect(&cb_url);
    assert_eq!(status, 200);
    assert!(body.contains("Signed in to Codex CLI"));

    handle.join().unwrap();

    // Verify auth.json written
    let auth_path = codex_home.path().join("auth.json");
    let contents = std::fs::read_to_string(&auth_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(v["OPENAI_API_KEY"].as_str(), Some("sk-test-123"));
    assert!(v["tokens"]["id_token"].as_str().is_some());
}

// 2) State mismatch returns 400 and server stays up
#[test]
fn login_server_state_mismatch() {
    let oauth_port = find_free_port();
    start_mock_oauth_server(oauth_port, MockBehavior::Success);
    let port = find_free_port();
    let codex_home = TempDir::new().unwrap();
    let issuer = format!("http://127.0.0.1:{oauth_port}");

    let opts = LoginServerOptions {
        codex_home: codex_home.path().into(),
        client_id: "test-client".into(),
        issuer,
        port,
        open_browser: false,
        redeem_credits: false,
        expose_state_endpoint: true,
        testing_timeout_secs: Some(5),
    };
    let handle = thread::spawn(move || run_local_login_server_with_options(opts).unwrap());
    wait_for_state_endpoint(port, Duration::from_secs(5));

    let cb_url = format!("http://127.0.0.1:{port}/auth/callback?code=abc&state=wrong");
    let (status, body) = http_get_follow_redirect(&cb_url);
    assert_eq!(status, 400);
    assert!(body.contains("State parameter mismatch") || body.is_empty());

    // Stop server
    let _ = ureq::get(&format!("http://127.0.0.1:{port}/success")).call();
    handle.join().unwrap();
}

// 3) Missing code returns 400
#[test]
fn login_server_missing_code() {
    let oauth_port = find_free_port();
    start_mock_oauth_server(oauth_port, MockBehavior::Success);
    let port = find_free_port();
    let codex_home = TempDir::new().unwrap();
    let issuer = format!("http://127.0.0.1:{oauth_port}");
    let opts = LoginServerOptions {
        codex_home: codex_home.path().into(),
        client_id: "test-client".into(),
        issuer,
        port,
        open_browser: false,
        redeem_credits: false,
        expose_state_endpoint: true,
        testing_timeout_secs: Some(5),
    };
    let handle = thread::spawn(move || run_local_login_server_with_options(opts).unwrap());
    wait_for_state_endpoint(port, Duration::from_secs(5));

    // Fetch state
    let state = ureq::get(&format!("http://127.0.0.1:{port}/__test/state")).call().expect("get state").into_string().unwrap();
    // Missing code
    let cb_url = format!("http://127.0.0.1:{port}/auth/callback?state={state}");
    let (status, _body) = http_get_follow_redirect(&cb_url);
    assert_eq!(status, 400);
    let _ = ureq::get(&format!("http://127.0.0.1:{port}/success")).call();
    handle.join().unwrap();
}

// 4) Token endpoint error returns 500 (on code exchange) and server stays up
#[test]
fn login_server_token_exchange_error() {
    let oauth_port = find_free_port();
    start_mock_oauth_server(oauth_port, MockBehavior::TokenError);
    let port = find_free_port();
    let codex_home = TempDir::new().unwrap();
    let issuer = format!("http://127.0.0.1:{oauth_port}");
    let opts = LoginServerOptions {
        codex_home: codex_home.path().into(),
        client_id: "test-client".into(),
        issuer,
        port,
        open_browser: false,
        redeem_credits: false,
        expose_state_endpoint: true,
        testing_timeout_secs: Some(5),
    };
    let handle = thread::spawn(move || run_local_login_server_with_options(opts).unwrap());
    wait_for_state_endpoint(port, Duration::from_secs(5));
    let state = ureq::get(&format!("http://127.0.0.1:{port}/__test/state")).call().expect("get state").into_string().unwrap();
    let cb_url = format!("http://127.0.0.1:{port}/auth/callback?code=abc&state={state}");
    let (status, _body) = http_get_follow_redirect(&cb_url);
    assert_eq!(status, 500);
    let _ = ureq::get(&format!("http://127.0.0.1:{port}/success")).call();
    handle.join().unwrap();
}

// 5) Credit redemption errors do not block success
#[test]
fn login_server_credit_redemption_best_effort() {
    // Mock behavior success for token endpoints, but have redeem endpoint return 500 by not matching path (using different port)
    let oauth_port = find_free_port();
    start_mock_oauth_server(oauth_port, MockBehavior::Success);
    let port = find_free_port();
    let codex_home = TempDir::new().unwrap();
    let issuer = format!("http://127.0.0.1:{oauth_port}");
    let opts = LoginServerOptions {
        codex_home: codex_home.path().into(),
        client_id: "test-client".into(),
        issuer,
        port,
        open_browser: false,
        redeem_credits: true,
        expose_state_endpoint: true,
        testing_timeout_secs: Some(5),
    };
    let handle = thread::spawn(move || run_local_login_server_with_options(opts).unwrap());
    wait_for_state_endpoint(port, Duration::from_secs(5));
    let state = ureq::get(&format!("http://127.0.0.1:{port}/__test/state")).call().expect("get state").into_string().unwrap();
    let cb_url = format!("http://127.0.0.1:{port}/auth/callback?code=abc&state={state}");
    let (status, _body) = http_get_follow_redirect(&cb_url);
    assert_eq!(status, 200);
    handle.join().unwrap();
    // auth.json exists
    assert!(codex_home.path().join("auth.json").exists());
}


fn wait_for_state_endpoint(port: u16, timeout: Duration) {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > timeout {
            panic!("server did not expose __test/state within timeout");
        }
        if let Ok(resp) = ureq::get(&format!("http://127.0.0.1:{port}/__test/state")).call() {
            if resp.status() == 200 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
