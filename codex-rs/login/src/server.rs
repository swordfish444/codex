//
use rand::RngCore;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tiny_http::Header;
use tiny_http::Method;
use tiny_http::Response;
use tiny_http::Server;
use url::Url;
use url::form_urlencoded;

use crate::auth_file::write_auth_file;
use crate::jwt_utils::parse_jwt_claims;
use crate::pkce::generate_pkce;
use crate::redeem::maybe_redeem_credits;
use crate::success_url::build_success_url;

pub const DEFAULT_PORT: u16 = 1455;
pub const DEFAULT_ISSUER: &str = "https://auth.openai.com";

pub const LOGIN_SUCCESS_HTML: &str = include_str!("./success_page.html");

#[derive(Debug, Deserialize)]
struct CodeExchangeResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

//

#[derive(Debug, Clone)]
pub struct LoginServerOptions {
    pub codex_home: PathBuf,
    pub client_id: String,
    pub issuer: String,
    pub port: u16,
    pub open_browser: bool,
    pub redeem_credits: bool,
    pub expose_state_endpoint: bool,
    /// When set, the server will auto-exit after the specified number of seconds by
    /// issuing an internal request to a test-only endpoint. Intended for CI/tests.
    pub testing_timeout_secs: Option<u64>,
    pub verbose: bool,
}

/// Extracts commonly used claims from ID and access tokens.
/// - account_id is taken from the ID token.
/// - org_id/project_id prefer ID token, falling back to access token.
/// - plan_type comes from the access token.
/// - needs_setup is computed from (completed_platform_onboarding, is_org_owner) with the same precedence as org/project.
fn extract_login_context(
    id_token: &str,
    access_token: &str,
) -> (
    Option<String>, // account_id
    Option<String>, // org_id
    Option<String>, // project_id
    bool,           // needs_setup
    Option<String>, // plan_type
) {
    let id_claims = parse_jwt_claims(id_token);
    let access_claims = parse_jwt_claims(access_token);
    let id_auth_claims = id_claims
        .get("https://api.openai.com/auth")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let access_auth_claims = access_claims
        .get("https://api.openai.com/auth")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));

    let account_id = id_auth_claims
        .get("chatgpt_account_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let org_id = id_auth_claims
        .get("organization_id")
        .and_then(|v| v.as_str())
        .or_else(|| access_auth_claims.get("organization_id").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let project_id = id_auth_claims
        .get("project_id")
        .and_then(|v| v.as_str())
        .or_else(|| access_auth_claims.get("project_id").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let completed_onboarding = id_auth_claims
        .get("completed_platform_onboarding")
        .and_then(|v| v.as_bool())
        .or_else(|| {
            access_auth_claims
                .get("completed_platform_onboarding")
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false);
    let is_org_owner = id_auth_claims
        .get("is_org_owner")
        .and_then(|v| v.as_bool())
        .or_else(|| access_auth_claims.get("is_org_owner").and_then(|v| v.as_bool()))
        .unwrap_or(false);
    let needs_setup = !completed_onboarding && is_org_owner;

    let plan_type = access_auth_claims
        .get("chatgpt_plan_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    (account_id, org_id, project_id, needs_setup, plan_type)
}

fn default_url_base(port: u16) -> String {
    format!("http://localhost:{port}")
}

#[allow(dead_code)]
pub fn run_local_login_server(codex_home: &Path, client_id: &str) -> std::io::Result<()> {
    let opts = LoginServerOptions {
        codex_home: codex_home.to_path_buf(),
        client_id: client_id.to_string(),
        issuer: DEFAULT_ISSUER.to_string(),
        port: DEFAULT_PORT,
        open_browser: true,
        redeem_credits: true,
        expose_state_endpoint: false,
        testing_timeout_secs: None,
        verbose: false,
    };
    run_local_login_server_with_options(opts)
}

pub fn run_local_login_server_with_options(opts: LoginServerOptions) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{}", opts.port);
    let server = Server::http(&addr).map_err(|e| std::io::Error::other(e.to_string()))?;

    let issuer = opts.issuer.clone();
    let token_endpoint = format!("{issuer}/oauth/token");
    let url_base = default_url_base(opts.port);

    let pkce = generate_pkce();
    let state = {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    };

    let redirect_uri = format!("{url_base}/auth/callback");
    let auth_url_str = format!("{issuer}/oauth/authorize");
    let mut auth_url =
        Url::parse(&auth_url_str).map_err(|e| std::io::Error::other(e.to_string()))?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &opts.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", "openid profile email offline_access")
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", &state);

    eprintln!("Starting local login server on {url_base}");
    // Try to open the browser, but ignore failures.
    if opts.open_browser {
        let _ = webbrowser::open(auth_url.as_str());
    }
    eprintln!(
        ". If your browser did not open, navigate to this URL to authenticate: \n\n{auth_url}"
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // If a testing timeout is configured, schedule an internal exit request so tests don't hang.
    if let Some(secs) = opts.testing_timeout_secs {
        let port = opts.port;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(secs));
            let _ = reqwest::blocking::get(format!("http://127.0.0.1:{port}/__test/exit"));
        });
    }

    // Main request loop
    'outer: loop {
        let request = match server.recv() {
            Ok(r) => r,
            Err(e) => {
                return Err(std::io::Error::other(e.to_string()));
            }
        };

        // Parse URL path and query
        let full = request.url().to_string();
        let (path, query) = match full.split_once('?') {
            Some((p, q)) => (p.to_string(), Some(q.to_string())),
            None => (full.clone(), None),
        };

        if opts.verbose {
            eprintln!("{} {}", request.method().as_str(), request.url());
        }

        match (request.method().clone(), path.as_str()) {
            (Method::Get, "/success") => {
                let mut resp = Response::from_string(LOGIN_SUCCESS_HTML).with_status_code(200);
                if let Ok(h) =
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                {
                    resp.add_header(h);
                }
                let _ = request.respond(resp);
                break 'outer;
            }
            (Method::Get, "/__test/exit") => {
                let _ = request.respond(Response::from_string("bye").with_status_code(200));
                break 'outer;
            }
            // Test-only helper to retrieve the current state, enabled via options.
            (Method::Get, "/__test/state") if opts.expose_state_endpoint => {
                let mut resp = Response::from_string(state.clone()).with_status_code(200);
                if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]) {
                    resp.add_header(h);
                }
                let _ = request.respond(resp);
            }
            (Method::Get, "/auth/callback") => {
                // Parse query params
                let params: HashMap<String, String> =
                    form_urlencoded::parse(query.as_deref().unwrap_or("").as_bytes())
                        .into_owned()
                        .collect();

                if params.get("state").map(|s| s.as_str()) != Some(state.as_str()) {
                    let _ = request.respond(
                        Response::from_string("State parameter mismatch").with_status_code(400),
                    );
                    continue;
                }
                let code = match params.get("code").cloned() {
                    Some(c) if !c.is_empty() => c,
                    _ => {
                        let _ = request.respond(
                            Response::from_string("Missing authorization code")
                                .with_status_code(400),
                        );
                        continue;
                    }
                };

                // 1) Authorization code -> tokens
                if opts.verbose {
                    eprintln!("POST {token_endpoint} (authorization_code)");
                }
                let token_resp = client
                    .post(&token_endpoint)
                    .form(&[
                        ("grant_type", "authorization_code"),
                        ("code", code.as_str()),
                        ("redirect_uri", redirect_uri.as_str()),
                        ("client_id", opts.client_id.as_str()),
                        ("code_verifier", pkce.code_verifier.as_str()),
                    ])
                    .send();
                let Ok(token_resp) = token_resp else {
                    if opts.verbose {
                        eprintln!("Token exchange failed: network error");
                    }
                    let _ = request.respond(
                        Response::from_string("Token exchange failed").with_status_code(500),
                    );
                    continue;
                };
                if !token_resp.status().is_success() {
                    let status = token_resp.status();
                    let body = token_resp.text().unwrap_or_default();
                    if opts.verbose {
                        eprintln!("Token exchange failed: status={status} body={body}");
                    }
                    let _ = request.respond(
                        Response::from_string("Token exchange failed").with_status_code(500),
                    );
                    continue;
                }
                let body_text = token_resp.text().unwrap_or_default();
                let tokens: CodeExchangeResponse = match serde_json::from_str(&body_text) {
                    Ok(v) => v,
                    Err(e) => {
                        if opts.verbose {
                            eprintln!("Token exchange failed: invalid JSON: {e} body={body_text}");
                        }
                        let _ = request.respond(
                            Response::from_string("Token exchange failed").with_status_code(500),
                        );
                        continue;
                    }
                };

                let (account_id, org_id, project_id, needs_setup, plan_type) =
                    extract_login_context(&tokens.id_token, &tokens.access_token);

                let api_key_opt: Option<String> = None;

                // Persist auth.json
                if let Err(e) = write_auth_file(
                    &opts.codex_home,
                    api_key_opt.clone(),
                    &tokens.id_token,
                    &tokens.access_token,
                    &tokens.refresh_token,
                    account_id,
                ) {
                    let _ = request.respond(
                        Response::from_string(format!("Unable to persist auth file: {e}"))
                            .with_status_code(500),
                    );
                    continue;
                }

                // Best-effort credits redemption
                if opts.redeem_credits {
                    maybe_redeem_credits(
                        &issuer,
                        &opts.client_id,
                        Some(&tokens.id_token),
                        &tokens.refresh_token,
                        &opts.codex_home,
                    );
                }

                // Build success URL and redirect
                let platform_url = if issuer == DEFAULT_ISSUER {
                    "https://platform.openai.com"
                } else {
                    "https://platform.api.openai.org"
                };
                let success_url = build_success_url(
                    &url_base,
                    Some(&tokens.id_token),
                    org_id.as_deref(),
                    project_id.as_deref(),
                    plan_type.as_deref(),
                    needs_setup,
                    platform_url,
                )
                .map_err(|e| std::io::Error::other(e.to_string()))?;

                let mut resp = Response::empty(302);
                let location_value = success_url.to_string();
                if let Ok(h) = Header::from_bytes(&b"Location"[..], location_value.as_str()) {
                    resp.add_header(h);
                }
                let _ = request.respond(resp);
            }
            _ => {
                let _ = request
                    .respond(Response::from_string("Endpoint not supported").with_status_code(404));
            }
        }
    }

    Ok(())
}

// -------- Headless testing helpers (no HTTP server) --------

#[derive(Debug, Clone)]
pub struct HeadlessOutcome {
    pub success_url: String,
    pub api_key: Option<String>,
}

pub trait Http {
    fn post_form(&self, url: &str, form: &[(String, String)])
    -> std::io::Result<serde_json::Value>;
    fn post_json(&self, url: &str, body: &serde_json::Value) -> std::io::Result<serde_json::Value>;
}

pub struct DefaultHttp(Client);
impl Default for DefaultHttp {
    fn default() -> Self {
        Self(Client::new())
    }
}
impl Http for DefaultHttp {
    fn post_form(
        &self,
        url: &str,
        form: &[(String, String)],
    ) -> std::io::Result<serde_json::Value> {
        let resp = self
            .0
            .post(url)
            .form(
                &form
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect::<Vec<_>>(),
            )
            .send()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let val = resp
            .json::<serde_json::Value>()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(val)
    }
    fn post_json(&self, url: &str, body: &serde_json::Value) -> std::io::Result<serde_json::Value> {
        let resp = self
            .0
            .post(url)
            .json(body)
            .send()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let val = resp
            .json::<serde_json::Value>()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(val)
    }
}

pub fn process_callback_headless(
    opts: &LoginServerOptions,
    expected_state: &str,
    incoming_state: &str,
    code_opt: Option<&str>,
    code_verifier: &str,
    http: &dyn Http,
) -> std::io::Result<HeadlessOutcome> {
    if incoming_state != expected_state {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "state mismatch",
        ));
    }
    let code = code_opt.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "missing authorization code",
        )
    })?;

    let token_endpoint = format!("{}/oauth/token", opts.issuer);
    let redirect_uri = format!("{}/auth/callback", default_url_base(opts.port));

    // 1) Code -> tokens
    let form = vec![
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code.to_string()),
        ("redirect_uri".to_string(), redirect_uri.clone()),
        ("client_id".to_string(), opts.client_id.clone()),
        ("code_verifier".to_string(), code_verifier.to_string()),
    ];
    let tokens_val = http.post_form(&token_endpoint, &form)?;
    let id_token = tokens_val["id_token"].as_str().unwrap_or("").to_string();
    let access_token = tokens_val["access_token"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let refresh_token = tokens_val["refresh_token"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if id_token.is_empty() || access_token.is_empty() || refresh_token.is_empty() {
        return Err(std::io::Error::other("token exchange failed"));
    }

    let (account_id, org_id, project_id, needs_setup, plan_type) =
        extract_login_context(&id_token, &access_token);

    let api_key = None;

    // Persist auth.json
    write_auth_file(
        &opts.codex_home,
        api_key.clone(),
        &id_token,
        &access_token,
        &refresh_token,
        account_id,
    )?;

    // Attempt credit redemption (best-effort)
    if opts.redeem_credits {
        let platform_url = if opts.issuer == DEFAULT_ISSUER {
            "https://api.openai.com"
        } else {
            "https://api.openai.org"
        };
        let redeem_url = format!("{platform_url}/v1/billing/redeem_credits");
        let _ = http.post_json(&redeem_url, &json!({"id_token": id_token}));
    }

    // Build success URL
    let base = default_url_base(opts.port);
    let platform_url = if opts.issuer == DEFAULT_ISSUER {
        "https://platform.openai.com"
    } else {
        "https://platform.api.openai.org"
    };
    let success_url = build_success_url(
        &base,
        Some(&id_token),
        org_id.as_deref(),
        project_id.as_deref(),
        plan_type.as_deref(),
        needs_setup,
        platform_url,
    )
    .map_err(|e| std::io::Error::other(e.to_string()))?;

    Ok(HeadlessOutcome {
        success_url: success_url.to_string(),
        api_key,
    })
}

//
