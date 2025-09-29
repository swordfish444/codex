use base64::Engine as _;
use chrono::Utc;
use reqwest::header::HeaderMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

pub fn set_user_agent_suffix(suffix: &str) {
    if let Ok(mut guard) = codex_core::default_client::USER_AGENT_SUFFIX.lock() {
        guard.replace(suffix.to_string());
    }
}

pub fn append_error_log(message: impl AsRef<str>) {
    let message = message.as_ref();
    let timestamp = Utc::now().to_rfc3339();

    if let Some(path) = log_file_path()
        && write_log_line(&path, &timestamp, message)
    {
        return;
    }

    let fallback = Path::new("error.log");
    let _ = write_log_line(fallback, &timestamp, message);
}

/// Normalize the configured base URL to a canonical form used by the backend client.
/// - trims trailing '/'
/// - appends '/backend-api' for ChatGPT hosts when missing
pub fn normalize_base_url(input: &str) -> String {
    let mut base_url = input.to_string();
    while base_url.ends_with('/') {
        base_url.pop();
    }
    if (base_url.starts_with("https://chatgpt.com")
        || base_url.starts_with("https://chat.openai.com"))
        && !base_url.contains("/backend-api")
    {
        base_url = format!("{base_url}/backend-api");
    }
    base_url
}

fn log_file_path() -> Option<PathBuf> {
    let mut log_dir = codex_core::config::find_codex_home().ok()?;
    log_dir.push("log");
    std::fs::create_dir_all(&log_dir).ok()?;
    Some(log_dir.join("codex-cloud-tasks.log"))
}

fn write_log_line(path: &Path, timestamp: &str, message: &str) -> bool {
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    match opts.open(path) {
        Ok(mut file) => {
            use std::io::Write as _;
            writeln!(file, "[{timestamp}] {message}").is_ok()
        }
        Err(_) => false,
    }
}

/// Extract the ChatGPT account id from a JWT token, when present.
pub fn extract_chatgpt_account_id(token: &str) -> Option<String> {
    let mut parts = token.split('.');
    let (_h, payload_b64, _s) = match (parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(p), Some(s)) if !h.is_empty() && !p.is_empty() && !s.is_empty() => (h, p, s),
        _ => return None,
    };
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&payload_bytes).ok()?;
    v.get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(|id| id.as_str())
        .map(str::to_string)
}

pub fn switch_to_branch(branch: &str) -> Result<(), String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("default branch name is empty".to_string());
    }

    if let Ok(current) = current_branch()
        && current == branch
    {
        append_error_log(format!("git.switch: already on branch {branch}"));
        return Ok(());
    }

    append_error_log(format!("git.switch: switching to branch {branch}"));
    match ensure_success(&["checkout", branch]) {
        Ok(()) => Ok(()),
        Err(err) => {
            append_error_log(format!("git.switch: checkout {branch} failed: {err}"));
            if ensure_success(&["rev-parse", "--verify", branch]).is_ok() {
                return Err(err);
            }
            if let Err(fetch_err) = ensure_success(&["fetch", "origin", branch]) {
                append_error_log(format!(
                    "git.switch: fetch origin/{branch} failed: {fetch_err}"
                ));
                return Err(err);
            }
            let tracking = format!("origin/{branch}");
            ensure_success(&["checkout", "-b", branch, &tracking]).map_err(|create_err| {
                append_error_log(format!(
                    "git.switch: checkout -b {branch} {tracking} failed: {create_err}"
                ));
                create_err
            })
        }
    }
}

fn current_branch() -> Result<String, String> {
    let output = run_git(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse --abbrev-ref failed: {}",
            format_command_failure(output, &["rev-parse", "--abbrev-ref", "HEAD"])
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ensure_success(args: &[&str]) -> Result<(), String> {
    let output = run_git(args)?;
    if output.status.success() {
        return Ok(());
    }
    Err(format_command_failure(output, args))
}

fn run_git(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("failed to launch git {}: {e}", join_args(args)))
}

fn format_command_failure(output: std::process::Output, args: &[&str]) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!(
        "git {} exited with status {}. stdout: {} stderr: {}",
        join_args(args),
        output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "<signal>".to_string()),
        stdout.trim(),
        stderr.trim()
    )
}

fn join_args(args: &[&str]) -> String {
    args.join(" ")
}

/// Build headers for ChatGPT-backed requests: `User-Agent`, optional `Authorization`,
/// and optional `ChatGPT-Account-Id`.
pub async fn build_chatgpt_headers() -> HeaderMap {
    use reqwest::header::AUTHORIZATION;
    use reqwest::header::HeaderName;
    use reqwest::header::HeaderValue;
    use reqwest::header::USER_AGENT;

    set_user_agent_suffix("codex_cloud_tasks_tui");
    let ua = codex_core::default_client::get_codex_user_agent();
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&ua).unwrap_or(HeaderValue::from_static("codex-cli")),
    );
    if let Ok(home) = codex_core::config::find_codex_home() {
        let am = codex_login::AuthManager::new(home);
        if let Some(auth) = am.auth()
            && let Ok(tok) = auth.get_token().await
            && !tok.is_empty()
        {
            let v = format!("Bearer {tok}");
            if let Ok(hv) = HeaderValue::from_str(&v) {
                headers.insert(AUTHORIZATION, hv);
            }
            if let Some(acc) = auth
                .get_account_id()
                .or_else(|| extract_chatgpt_account_id(&tok))
                && let Ok(name) = HeaderName::from_bytes(b"ChatGPT-Account-Id")
                && let Ok(hv) = HeaderValue::from_str(&acc)
            {
                headers.insert(name, hv);
            }
        }
    }
    headers
}
