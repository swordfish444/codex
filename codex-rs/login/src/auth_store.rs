use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::path::PathBuf;

use crate::token_data::TokenData;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct AuthDotJson {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

pub fn get_auth_file(codex_home: &Path) -> PathBuf {
    codex_home.join("auth.json")
}

/// Delete the auth.json file inside `codex_home` if it exists. Returns `Ok(true)`
/// if a file was removed, `Ok(false)` if no auth file was present.
pub fn logout(codex_home: &Path) -> std::io::Result<bool> {
    let auth_file = get_auth_file(codex_home);
    match std::fs::remove_file(&auth_file) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

/// Attempt to read and deserialize the `auth.json` file at the given path.
/// Returns the full AuthDotJson structure.
pub fn try_read_auth_json(auth_file: &Path) -> std::io::Result<AuthDotJson> {
    let mut file = File::open(auth_file)?;
    let mut contents = String::new();
    use std::io::Read as _;
    file.read_to_string(&mut contents)?;
    let auth_dot_json: AuthDotJson = serde_json::from_str(&contents)?;
    Ok(auth_dot_json)
}

pub(crate) fn write_auth_json(
    auth_file: &Path,
    auth_dot_json: &AuthDotJson,
) -> std::io::Result<()> {
    let json_data = serde_json::to_string_pretty(auth_dot_json)?;
    let mut options = OpenOptions::new();
    options.truncate(true).write(true).create(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options.open(auth_file)?;
    use std::io::Write as _;
    file.write_all(json_data.as_bytes())?;
    file.flush()?;
    Ok(())
}

pub fn login_with_api_key(codex_home: &Path, api_key: &str) -> std::io::Result<()> {
    let auth_dot_json = AuthDotJson {
        openai_api_key: Some(api_key.to_string()),
        tokens: None,
        last_refresh: None,
    };
    write_auth_json(&get_auth_file(codex_home), &auth_dot_json)
}

pub(crate) fn update_tokens(
    auth_file: &Path,
    id_token: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
) -> std::io::Result<AuthDotJson> {
    let mut prior_access: Option<String> = None;
    let mut prior_refresh: Option<String> = None;
    let mut auth = match try_read_auth_json(auth_file) {
        Ok(a) => a,
        Err(_) => {
            // Try to salvage existing access/refresh from raw JSON on disk
            if let Ok(mut f) = File::open(auth_file) {
                let mut contents = String::new();
                use std::io::Read as _;
                if f.read_to_string(&mut contents).is_ok() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&contents) {
                        prior_access = val
                            .get("tokens")
                            .and_then(|t| t.get("access_token"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        prior_refresh = val
                            .get("tokens")
                            .and_then(|t| t.get("refresh_token"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }
            }
            AuthDotJson {
                openai_api_key: None,
                tokens: None,
                last_refresh: None,
            }
        }
    };
    let now = Utc::now();
    auth.last_refresh = Some(now);

    let new_tokens = match auth.tokens.take() {
        Some(mut tokens) => {
            tokens.id_token_raw = id_token;
            if let Some(a) = access_token.clone() {
                tokens.access_token = a;
            }
            if let Some(r) = refresh_token.clone() {
                tokens.refresh_token = r;
            }
            // Re-parse id_token_raw into parsed fields
            tokens.id_token = crate::token_data::parse_id_token(&tokens.id_token_raw)
                .map_err(std::io::Error::other)?;
            tokens
        }
        None => {
            // Construct fresh TokenData from provided values
            let a = access_token
                .or_else(|| prior_access.clone())
                .ok_or_else(|| std::io::Error::other("missing access_token"))?;
            let r = refresh_token
                .or_else(|| prior_refresh.clone())
                .ok_or_else(|| std::io::Error::other("missing refresh_token"))?;
            crate::token_data::TokenData::from_raw(id_token, a, r, None)
                .map_err(std::io::Error::other)?
        }
    };

    auth.tokens = Some(new_tokens);
    write_auth_json(auth_file, &auth)?;
    try_read_auth_json(auth_file)
}
