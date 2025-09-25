#![deny(clippy::unwrap_used, clippy::expect_used)]

use codex_backend_client::Client as BackendClient;
use codex_cloud_tasks::util::extract_chatgpt_account_id;
use codex_cloud_tasks::util::normalize_base_url;
use codex_core::config::find_codex_home;
use codex_core::default_client::get_codex_user_agent;
use codex_login::AuthManager;
use codex_login::AuthMode;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Base URL (default to ChatGPT backend API) and normalize to canonical form
    let raw_base = std::env::var("CODEX_CLOUD_TASKS_BASE_URL")
        .unwrap_or_else(|_| "https://chatgpt.com/backend-api".to_string());
    let base_url = normalize_base_url(&raw_base);
    println!("base_url: {base_url}");
    let path_style = if base_url.contains("/backend-api") {
        "wham"
    } else {
        "codex-api"
    };
    println!("path_style: {path_style}");

    // Locate CODEX_HOME and try to load ChatGPT auth
    let codex_home = match find_codex_home() {
        Ok(p) => {
            println!("codex_home: {}", p.display());
            Some(p)
        }
        Err(e) => {
            println!("codex_home: <not found> ({e})");
            None
        }
    };

    // Build backend client with UA
    let ua = get_codex_user_agent(Some("codex_cloud_tasks_conncheck"));
    let mut client = BackendClient::new(base_url.clone())?.with_user_agent(ua);

    // Attach bearer token if available from ChatGPT auth
    let mut have_auth = false;
    if let Some(home) = codex_home {
        let authm = AuthManager::new(
            home,
            AuthMode::ChatGPT,
            "codex_cloud_tasks_conncheck".to_string(),
        );
        if let Some(auth) = authm.auth() {
            match auth.get_token().await {
                Ok(token) if !token.is_empty() => {
                    have_auth = true;
                    println!("auth: ChatGPT token present ({} chars)", token.len());
                    // Add Authorization header
                    client = client.with_bearer_token(&token);

                    // Attempt to extract ChatGPT account id from the JWT and set header.
                    if let Some(account_id) = extract_chatgpt_account_id(&token) {
                        println!("auth: ChatGPT-Account-Id: {account_id}");
                        client = client.with_chatgpt_account_id(account_id);
                    } else if let Some(acc) = auth.get_account_id() {
                        // Fallback: some older auth.jsons persist account_id
                        println!("auth: ChatGPT-Account-Id (from auth.json): {acc}");
                        client = client.with_chatgpt_account_id(acc);
                    }
                }
                Ok(_) => {
                    println!("auth: ChatGPT token empty");
                }
                Err(e) => {
                    println!("auth: failed to load ChatGPT token: {e}");
                }
            }
        } else {
            println!("auth: no ChatGPT auth.json");
        }
    }

    if !have_auth {
        println!("note: Online endpoints typically require ChatGPT sign-in. Run: `codex login`");
    }

    // Attempt the /list call with a short timeout to avoid hanging
    match path_style {
        "wham" => println!("request: GET /wham/tasks/list?limit=5&task_filter=current"),
        _ => println!("request: GET /api/codex/tasks/list?limit=5&task_filter=current"),
    }
    let fut = client.list_tasks(Some(5), Some("current"), None);
    let res = tokio::time::timeout(Duration::from_secs(30), fut).await;
    match res {
        Err(_) => {
            println!("error: request timed out after 30s");
            std::process::exit(2);
        }
        Ok(Err(e)) => {
            // backend-client includes HTTP status and body in errors.
            println!("error: {e}");
            std::process::exit(1);
        }
        Ok(Ok(list)) => {
            println!("ok: received {} tasks", list.items.len());
            for item in list.items.iter().take(5) {
                println!("- {} — {}", item.id, item.title);
            }
            // Keep output concise; omit full JSON payload to stay readable.
        }
    }

    Ok(())
}
