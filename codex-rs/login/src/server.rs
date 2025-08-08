use base64::Engine as _;
use chrono::Utc;
use rand::RngCore;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server};
use url::Url;
use std::path::PathBuf;

const DEFAULT_PORT: u16 = 1455;
const DEFAULT_ISSUER: &str = "https://auth.openai.com";

// Copied from the Python HTML to keep UX consistent.
pub const LOGIN_SUCCESS_HTML: &str = include_str!("./success_page.html");

#[derive(Debug, Clone)]
pub struct PkceCodes {
    code_verifier: String,
    code_challenge: String,
}

fn generate_pkce() -> PkceCodes {
    // Equivalent to Python's secrets.token_hex(64)
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    let code_verifier = hex::encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

#[derive(Debug, Deserialize)]
struct CodeExchangeResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
}

fn now_rfc3339_z() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

fn decode_jwt_payload_segment(segment_b64: &str) -> serde_json::Value {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(segment_b64)
        .ok();
    decoded
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .unwrap_or(serde_json::Value::Object(Default::default()))
}

fn parse_jwt_claims(token: &str) -> serde_json::Value {
    // Expect three segments: header.payload.signature
    let mut parts = token.split('.');
    let _header = parts.next();
    let payload = parts.next();
    let _sig = parts.next();
    match payload {
        Some(p) if !p.is_empty() => decode_jwt_payload_segment(p),
        _ => serde_json::Value::Object(Default::default()),
    }
}

fn write_auth_file(
    codex_home: &Path,
    api_key: Option<String>,
    id_token: &str,
    access_token: &str,
    refresh_token: &str,
    account_id: Option<String>,
) -> std::io::Result<()> {
    std::fs::create_dir_all(codex_home)?;
    let auth_path = codex_home.join("auth.json");

    let contents = json!({
        "OPENAI_API_KEY": api_key,
        "tokens": {
            "id_token": id_token,
            "access_token": access_token,
            "refresh_token": refresh_token,
            "account_id": account_id,
        },
        "last_refresh": now_rfc3339_z(),
    })
    .to_string();

    let mut opts = OpenOptions::new();
    opts.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        opts.mode(0o600);
    }
    let mut f = opts.open(auth_path)?;
    use std::io::Write;
    f.write_all(contents.as_bytes())?;
    f.flush()
}

fn maybe_redeem_credits(
    issuer: &str,
    client_id: &str,
    id_token_opt: Option<&str>,
    refresh_token: &str,
    codex_home: &Path,
) {
    // Best-effort: any error should not abort the login flow.
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build();
    let Ok(client) = client else { return };

    // Parse initial ID token claims and check expiration.
    let mut id_token = id_token_opt.unwrap_or("").to_string();
    let mut claims = parse_jwt_claims(&id_token);

    let mut token_expired = true;
    if let Some(exp) = claims.get("exp").and_then(|v| v.as_i64()) {
        let now_ms = (Utc::now().timestamp_millis()) as i64;
        token_expired = now_ms >= exp * 1000;
    }

    if token_expired {
        eprintln!("Refreshing credentials...");
        #[derive(serde::Serialize)]
        struct RefreshReq<'a> {
            client_id: &'a str,
            grant_type: &'a str,
            refresh_token: &'a str,
            scope: &'a str,
        }
        let body = RefreshReq {
            client_id,
            grant_type: "refresh_token",
            refresh_token,
            scope: "openid profile email",
        };
        let resp = client
            .post("https://auth.openai.com/oauth/token")
            .json(&body)
            .send();
        let Ok(resp) = resp else { return };
        let Ok(val) = resp.json::<serde_json::Value>() else { return };
        let new_id_token = val.get("id_token").and_then(|v| v.as_str()).map(|s| s.to_string());
        let new_refresh_token = val
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let (Some(new_id), Some(new_refresh)) = (new_id_token, new_refresh_token) {
            // Update file on disk with new tokens.
            // Read, modify, write.
            let path = codex_home.join("auth.json");
            if let Ok(mut existing) = std::fs::read_to_string(&path) {
                if let Ok(mut obj) = serde_json::from_str::<serde_json::Value>(&existing) {
                    obj["tokens"]["id_token"] = serde_json::Value::String(new_id.clone());
                    obj["tokens"]["refresh_token"] = serde_json::Value::String(new_refresh.clone());
                    obj["tokens"]["last_refresh"] = serde_json::Value::String(now_rfc3339_z());
                    existing = serde_json::to_string_pretty(&obj).unwrap_or(existing);
                    let _ = std::fs::write(&path, existing);
                    id_token = new_id;
                    claims = parse_jwt_claims(&id_token);
                }
            }
        } else {
            return;
        }
    }

    // Eligibility checks
    let auth_claims = claims
        .get("https://api.openai.com/auth")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let needs_setup = {
        let completed = auth_claims
            .get("completed_platform_onboarding")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_owner = auth_claims
            .get("is_org_owner")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        !completed && is_owner
    };
    if needs_setup {
        eprintln!("Only users with Plus or Pro subscriptions can redeem free API credits.");
        return;
    }
    let plan_type = auth_claims
        .get("chatgpt_plan_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if plan_type != "plus" && plan_type != "pro" {
        eprintln!("Only users with Plus or Pro subscriptions can redeem free API credits.");
        return;
    }

    let api_host = if issuer == DEFAULT_ISSUER {
        "https://api.openai.com"
    } else {
        "https://api.openai.org"
    };

    let payload = json!({"id_token": id_token});
    let resp = client
        .post(format!("{api_host}/v1/billing/redeem_credits"))
        .json(&payload)
        .send();
    match resp {
        Ok(r) => match r.json::<serde_json::Value>() {
            Ok(val) => {
                let granted = val
                    .get("granted_chatgpt_subscriber_api_credits")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if granted > 0 {
                    let amount = if plan_type == "plus" { "$5" } else { "$50" };
                    eprintln!(
                        "Thanks for being a ChatGPT {} subscriber! If you haven't already redeemed, you should receive {} in API credits.",
                        if plan_type == "plus" { "Plus" } else { "Pro" },
                        amount
                    );
                } else {
                    eprintln!("It looks like no credits were granted: {}", val);
                }
            }
            Err(_) => {}
        },
        Err(_) => {}
    }
}

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
}

fn default_url_base(port: u16) -> String { format!("http://127.0.0.1:{port}") }

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
    };
    run_local_login_server_with_options(opts)
}

pub fn run_local_login_server_with_options(opts: LoginServerOptions) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{}", opts.port);
    let server = Server::http(&addr)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let issuer = opts.issuer.clone();
    let token_endpoint = format!("{}/oauth/token", issuer);
    let url_base = default_url_base(opts.port);

    let pkce = generate_pkce();
    let state = {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    };

    let redirect_uri = format!("{}/auth/callback", url_base);
    let mut auth_url = Url::parse(&format!("{}/oauth/authorize", issuer)).unwrap();
    auth_url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &opts.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", "openid profile email offline_access")
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", &state);

    eprintln!("Starting local login server on {}", url_base);
    // Try to open the browser, but ignore failures.
    if opts.open_browser {
        let _ = webbrowser::open(auth_url.as_str());
    }
    eprintln!(
        ". If your browser did not open, navigate to this URL to authenticate: \n\n{}",
        auth_url
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

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
            Err(e) => return Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())),
        };

        // Parse URL path and query
        let full = request.url().to_string();
        let (path, query) = match full.split_once('?') {
            Some((p, q)) => (p.to_string(), Some(q.to_string())),
            None => (full.clone(), None),
        };

        match (request.method().clone(), path.as_str()) {
            (Method::Get, "/success") => {
                let mut resp = Response::from_string(LOGIN_SUCCESS_HTML)
                    .with_status_code(200);
                resp.add_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
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
                resp.add_header(Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap());
                let _ = request.respond(resp);
            }
            (Method::Get, "/auth/callback") => {
                // Parse query params
                let params: HashMap<String, String> = query
                    .as_deref()
                    .unwrap_or("")
                    .split('&')
                    .filter_map(|kv| kv.split_once('='))
                    .filter_map(|(k, v)| {
                        let kk = urlencoding::decode(k).ok()?.into_owned();
                        let vv = urlencoding::decode(v).ok()?.into_owned();
                        Some((kk, vv))
                    })
                    .collect();

                if params.get("state").map(|s| s.as_str()) != Some(state.as_str()) {
                    let _ = request.respond(Response::from_string("State parameter mismatch").with_status_code(400));
                    continue;
                }
                let code = match params.get("code").cloned() {
                    Some(c) if !c.is_empty() => c,
                    _ => {
                        let _ = request.respond(Response::from_string("Missing authorization code").with_status_code(400));
                        continue;
                    }
                };

                // 1) Authorization code -> tokens
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
                    let _ = request.respond(Response::from_string("Token exchange failed").with_status_code(500));
                    continue;
                };
                let Ok(tokens) = token_resp.json::<CodeExchangeResponse>() else {
                    let _ = request.respond(Response::from_string("Token exchange failed").with_status_code(500));
                    continue;
                };

                // Extract account_id from id_token claims
                let id_claims = parse_jwt_claims(&tokens.id_token);
                let auth_claims = id_claims
                    .get("https://api.openai.com/auth")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let account_id = auth_claims
                    .get("chatgpt_account_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // Parse access token claims to compute redirect target
                let access_claims = parse_jwt_claims(&tokens.access_token);
                let access_auth_claims = access_claims
                    .get("https://api.openai.com/auth")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let org_id = access_auth_claims.get("organization_id").and_then(|v| v.as_str());
                let project_id = access_auth_claims.get("project_id").and_then(|v| v.as_str());
                let completed_onboarding = access_auth_claims
                    .get("completed_platform_onboarding")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let is_org_owner = access_auth_claims
                    .get("is_org_owner")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let plan_type = access_auth_claims
                    .get("chatgpt_plan_type")
                    .and_then(|v| v.as_str());
                let needs_setup = !completed_onboarding && is_org_owner;

                // 2) Token exchange for API key
                let today = Utc::now().format("%Y-%m-%d").to_string();
                let random_id = {
                    let mut bytes = [0u8; 6];
                    rand::thread_rng().fill_bytes(&mut bytes);
                    hex::encode(bytes)
                };
                let token_x_resp = client
                    .post(&token_endpoint)
                    .form(&[
                        ("grant_type", "urn:ietf:params:oauth:grant-type:token-exchange"),
                        ("client_id", opts.client_id.as_str()),
                        ("requested_token", "openai-api-key"),
                        ("subject_token", tokens.id_token.as_str()),
                        ("subject_token_type", "urn:ietf:params:oauth:token-type:id_token"),
                        (
                            "name",
                            format!("Codex CLI [auto-generated] ({today}) [{random_id}]").as_str(),
                        ),
                    ])
                    .send();
                let Ok(token_x_resp) = token_x_resp else {
                    let _ = request.respond(Response::from_string("Token exchange failed").with_status_code(500));
                    continue;
                };
                let Ok(token_x) = token_x_resp.json::<TokenExchangeResponse>() else {
                    let _ = request.respond(Response::from_string("Token exchange failed").with_status_code(500));
                    continue;
                };

                // Persist auth.json
                if let Err(e) = write_auth_file(
                    &opts.codex_home,
                    Some(token_x.access_token.clone()),
                    &tokens.id_token,
                    &tokens.access_token,
                    &tokens.refresh_token,
                    account_id,
                ) {
                    let _ = request.respond(Response::from_string(format!("Unable to persist auth file: {e}")).with_status_code(500));
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
                let mut success_url = Url::parse(&format!("{}/success", url_base)).unwrap();
                if let Some(id_tok) = Some(&tokens.id_token) {
                    success_url.query_pairs_mut().append_pair("id_token", id_tok);
                }
                if let Some(org) = org_id { success_url.query_pairs_mut().append_pair("org_id", org); }
                if let Some(proj) = project_id { success_url.query_pairs_mut().append_pair("project_id", proj); }
                if let Some(pt) = plan_type { success_url.query_pairs_mut().append_pair("plan_type", pt); }
                success_url
                    .query_pairs_mut()
                    .append_pair("needs_setup", if needs_setup { "true" } else { "false" })
                    .append_pair("platform_url", platform_url);

                let mut resp = Response::empty(302);
                resp.add_header(Header::from_bytes(&b"Location"[..], success_url.as_str()).unwrap());
                let _ = request.respond(resp);
            }
            _ => {
                let _ = request.respond(Response::from_string("Endpoint not supported").with_status_code(404));
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
    fn post_form(&self, url: &str, form: &[(String, String)]) -> std::io::Result<serde_json::Value>;
    fn post_json(&self, url: &str, body: &serde_json::Value) -> std::io::Result<serde_json::Value>;
}

pub struct DefaultHttp(Client);
impl Default for DefaultHttp {
    fn default() -> Self {
        let c = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            .unwrap();
        Self(c)
    }
}
impl Http for DefaultHttp {
    fn post_form(&self, url: &str, form: &[(String, String)]) -> std::io::Result<serde_json::Value> {
        let resp = self
            .0
            .post(url)
            .form(&form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<Vec<_>>())
            .send()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let val = resp
            .json::<serde_json::Value>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(val)
    }
    fn post_json(&self, url: &str, body: &serde_json::Value) -> std::io::Result<serde_json::Value> {
        let resp = self
            .0
            .post(url)
            .json(body)
            .send()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let val = resp
            .json::<serde_json::Value>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
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
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing authorization code")
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
    let access_token = tokens_val["access_token"].as_str().unwrap_or("").to_string();
    let refresh_token = tokens_val["refresh_token"].as_str().unwrap_or("").to_string();
    if id_token.is_empty() || access_token.is_empty() || refresh_token.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "token exchange failed",
        ));
    }

    // Extract claims
    let id_claims = parse_jwt_claims(&id_token);
    let account_id = id_claims
        .get("https://api.openai.com/auth")
        .and_then(|v| v.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let access_claims = parse_jwt_claims(&access_token);
    let access_auth = access_claims
        .get("https://api.openai.com/auth")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let org_id = access_auth.get("organization_id").and_then(|v| v.as_str());
    let project_id = access_auth.get("project_id").and_then(|v| v.as_str());
    let completed = access_auth
        .get("completed_platform_onboarding")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let is_owner = access_auth
        .get("is_org_owner")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let plan_type = access_auth
        .get("chatgpt_plan_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let needs_setup = !completed && is_owner;

    // 2) Token exchange -> API key
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let random_id = {
        let mut bytes = [0u8; 6];
        rand::thread_rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    };
    let exchange_form = vec![
        (
            "grant_type".to_string(),
            "urn:ietf:params:oauth:grant-type:token-exchange".to_string(),
        ),
        ("client_id".to_string(), opts.client_id.clone()),
        ("requested_token".to_string(), "openai-api-key".to_string()),
        ("subject_token".to_string(), id_token.clone()),
        (
            "subject_token_type".to_string(),
            "urn:ietf:params:oauth:token-type:id_token".to_string(),
        ),
        (
            "name".to_string(),
            format!("Codex CLI [auto-generated] ({today}) [{random_id}]").to_string(),
        ),
    ];
    let exchange_val = http.post_form(&token_endpoint, &exchange_form)?;
    let api_key = exchange_val["access_token"].as_str().map(|s| s.to_string());

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
    let mut success_url = Url::parse(&format!("{base}/success")).unwrap();
    success_url.query_pairs_mut().append_pair("id_token", &id_token);
    if let Some(org) = org_id {
        success_url.query_pairs_mut().append_pair("org_id", org);
    }
    if let Some(proj) = project_id {
        success_url
            .query_pairs_mut()
            .append_pair("project_id", proj);
    }
    if !plan_type.is_empty() {
        success_url
            .query_pairs_mut()
            .append_pair("plan_type", plan_type);
    }
    success_url
        .query_pairs_mut()
        .append_pair("needs_setup", if needs_setup { "true" } else { "false" })
        .append_pair(
            "platform_url",
            if opts.issuer == DEFAULT_ISSUER {
                "https://platform.openai.com"
            } else {
                "https://platform.api.openai.org"
            },
        );

    Ok(HeadlessOutcome {
        success_url: success_url.into_string(),
        api_key,
    })
}


