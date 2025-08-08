use chrono::Utc;
use serde_json::json;
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(crate) fn now_rfc3339_z() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

pub(crate) fn write_auth_file(
    codex_home: &Path,
    api_key: Option<String>,
    id_token: &str,
    access_token: &str,
    refresh_token: &str,
    account_id: Option<String>,
) -> std::io::Result<()> {
    std::fs::create_dir_all(codex_home)?;
    let auth_path = codex_home.join("auth.json");

    let contents = serde_json::to_string_pretty(&json!({
        "OPENAI_API_KEY": api_key,
        "tokens": {
            "id_token": id_token,
            "access_token": access_token,
            "refresh_token": refresh_token,
            "account_id": account_id,
        },
        "last_refresh": now_rfc3339_z(),
    }))
    .unwrap_or_else(|_| "{}".to_string());

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


