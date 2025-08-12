#![expect(clippy::expect_used, clippy::unwrap_used)]
use crate::auth::AuthMode;
use crate::auth::CodexAuth;
use crate::auth::load_auth;
use crate::auth_store::get_auth_file;
use crate::auth_store::logout;
use crate::token_data::IdTokenInfo;
use crate::token_data::KnownPlan;
use crate::token_data::PlanType;
use crate::token_data::parse_id_token;
use base64::Engine;
use pretty_assertions::assert_eq;
use serde::Serialize;
use serde_json::json;
use std::path::Path;
use tempfile::tempdir;

const LAST_REFRESH: &str = "2025-08-06T20:41:36.232376Z";

// moved to integration tests in tests/api_key_login.rs

#[test]
fn loads_from_env_var_if_env_var_exists() {
    let dir = tempdir().unwrap();
    let env_var = std::env::var(crate::OPENAI_API_KEY_ENV_VAR);
    if let Ok(env_var) = env_var {
        let auth = load_auth(dir.path(), true).unwrap().unwrap();
        assert_eq!(auth.mode, AuthMode::ApiKey);
        assert_eq!(auth.api_key, Some(env_var));
    }
}

#[tokio::test]
async fn pro_account_with_no_api_key_uses_chatgpt_auth() {
    let codex_home = tempdir().unwrap();
    write_auth_file(
        AuthFileParams {
            openai_api_key: None,
            chatgpt_plan_type: "pro".to_string(),
        },
        codex_home.path(),
    )
    .expect("failed to write auth file");

    let CodexAuth {
        api_key,
        mode,
        auth_dot_json,
        auth_file: _,
    } = load_auth(codex_home.path(), false).unwrap().unwrap();
    assert_eq!(None, api_key);
    assert_eq!(AuthMode::ChatGPT, mode);

    let guard = auth_dot_json.lock().unwrap();
    let actual = guard.as_ref().expect("AuthDotJson should exist");
    assert_eq!(actual.openai_api_key, None);
    let tokens = actual.tokens.as_ref().expect("tokens should exist");
    assert_eq!(
        tokens.id_token,
        IdTokenInfo {
            email: Some("user@example.com".to_string()),
            chatgpt_plan_type: Some(PlanType::Known(KnownPlan::Pro)),
        }
    );
    assert_eq!(tokens.access_token, "test-access-token".to_string());
    assert_eq!(tokens.refresh_token, "test-refresh-token".to_string());
    assert_eq!(tokens.account_id, None);
    assert_eq!(
        actual.last_refresh,
        Some(
            chrono::DateTime::parse_from_rfc3339(LAST_REFRESH)
                .unwrap()
                .with_timezone(&chrono::Utc)
        )
    );
}

/// Even if the OPENAI_API_KEY is set in auth.json, if the plan is not in
/// [`TokenData::is_plan_that_should_use_api_key`], it should use
/// [`AuthMode::ChatGPT`].
#[tokio::test]
async fn pro_account_with_api_key_still_uses_chatgpt_auth() {
    let codex_home = tempdir().unwrap();
    write_auth_file(
        AuthFileParams {
            openai_api_key: Some("sk-test-key".to_string()),
            chatgpt_plan_type: "pro".to_string(),
        },
        codex_home.path(),
    )
    .expect("failed to write auth file");

    let CodexAuth {
        api_key,
        mode,
        auth_dot_json,
        auth_file: _,
    } = load_auth(codex_home.path(), false).unwrap().unwrap();
    assert_eq!(None, api_key);
    assert_eq!(AuthMode::ChatGPT, mode);

    let guard = auth_dot_json.lock().unwrap();
    let actual = guard.as_ref().expect("AuthDotJson should exist");
    assert_eq!(actual.openai_api_key, None);
    let tokens = actual.tokens.as_ref().expect("tokens should exist");
    assert_eq!(
        tokens.id_token,
        IdTokenInfo {
            email: Some("user@example.com".to_string()),
            chatgpt_plan_type: Some(PlanType::Known(KnownPlan::Pro)),
        }
    );
    assert_eq!(tokens.access_token, "test-access-token".to_string());
    assert_eq!(tokens.refresh_token, "test-refresh-token".to_string());
    assert_eq!(tokens.account_id, None);
    assert_eq!(
        actual.last_refresh,
        Some(
            chrono::DateTime::parse_from_rfc3339(LAST_REFRESH)
                .unwrap()
                .with_timezone(&chrono::Utc)
        )
    );
}

/// If the OPENAI_API_KEY is set in auth.json and it is an enterprise
/// account, then it should use [`AuthMode::ApiKey`].
#[tokio::test]
async fn enterprise_account_with_api_key_uses_chatgpt_auth() {
    let codex_home = tempdir().unwrap();
    write_auth_file(
        AuthFileParams {
            openai_api_key: Some("sk-test-key".to_string()),
            chatgpt_plan_type: "enterprise".to_string(),
        },
        codex_home.path(),
    )
    .expect("failed to write auth file");

    let CodexAuth {
        api_key,
        mode,
        auth_dot_json,
        auth_file: _,
    } = load_auth(codex_home.path(), false).unwrap().unwrap();
    assert_eq!(Some("sk-test-key".to_string()), api_key);
    assert_eq!(AuthMode::ApiKey, mode);

    let guard = auth_dot_json.lock().expect("should unwrap");
    assert!(guard.is_none(), "auth_dot_json should be None");
}

struct AuthFileParams {
    openai_api_key: Option<String>,
    chatgpt_plan_type: String,
}

fn write_auth_file(params: AuthFileParams, codex_home: &Path) -> std::io::Result<()> {
    let auth_file = get_auth_file(codex_home);
    #[derive(Serialize)]
    struct Header {
        alg: &'static str,
        typ: &'static str,
    }
    let header = Header {
        alg: "none",
        typ: "JWT",
    };
    let payload = serde_json::json!({
        "email": "user@example.com",
        "email_verified": true,
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "bc3618e3-489d-4d49-9362-1561dc53ba53",
            "chatgpt_plan_type": params.chatgpt_plan_type,
            "chatgpt_user_id": "user-12345",
            "user_id": "user-12345",
        }
    });
    let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
    let header_b64 = b64(&serde_json::to_vec(&header)?);
    let payload_b64 = b64(&serde_json::to_vec(&payload)?);
    let signature_b64 = b64(b"sig");
    let fake_jwt = format!("{header_b64}.{payload_b64}.{signature_b64}");

    let auth_json_data = json!({
        "OPENAI_API_KEY": params.openai_api_key,
        "tokens": {
            "id_token": fake_jwt,
            "access_token": "test-access-token",
            "refresh_token": "test-refresh-token"
        },
        "last_refresh": LAST_REFRESH,
    });
    let auth_json = serde_json::to_string_pretty(&auth_json_data)?;
    std::fs::write(auth_file, auth_json)
}

#[test]
fn id_token_info_handles_missing_fields() {
    // Payload without email or plan should yield None values.
    let header = serde_json::json!({"alg": "none", "typ": "JWT"});
    let payload = serde_json::json!({"sub": "123"});
    let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&header).unwrap());
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&payload).unwrap());
    let signature_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"sig");
    let jwt = format!("{header_b64}.{payload_b64}.{signature_b64}");

    let info = parse_id_token(&jwt).expect("should parse");
    assert!(info.email.is_none());
    assert!(info.chatgpt_plan_type.is_none());
}

#[tokio::test]
async fn loads_api_key_from_auth_json() {
    let dir = tempdir().unwrap();
    let auth_file = dir.path().join("auth.json");
    std::fs::write(
        auth_file,
        r#"
        {
            "OPENAI_API_KEY": "sk-test-key",
            "tokens": null,
            "last_refresh": null
        }
        "#,
    )
    .unwrap();

    let auth = load_auth(dir.path(), false).unwrap().unwrap();
    assert_eq!(auth.mode, AuthMode::ApiKey);
    assert_eq!(auth.api_key, Some("sk-test-key".to_string()));

    assert!(auth.get_token_data().await.is_err());
}

#[test]
fn logout_removes_auth_file() -> Result<(), std::io::Error> {
    let dir = tempdir()?;
    crate::auth_store::login_with_api_key(dir.path(), "sk-test-key")?;
    assert!(dir.path().join("auth.json").exists());
    let removed = logout(dir.path())?;
    assert!(removed);
    assert!(!dir.path().join("auth.json").exists());
    Ok(())
}

#[test]
fn update_tokens_preserves_id_token_as_string() {
    let dir = tempdir().unwrap();
    let auth_file = crate::auth_store::get_auth_file(dir.path());

    // Write an initial auth.json with a tokens object
    let initial = serde_json::json!({
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": "old-id-token",
            "access_token": "a1",
            "refresh_token": "r1"
        },
        "last_refresh": LAST_REFRESH
    });
    std::fs::write(&auth_file, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    // Build a valid-looking JWT (URL-safe base64 header.payload.signature)
    #[derive(Serialize)]
    struct Header {
        alg: &'static str,
        typ: &'static str,
    }
    let header = Header {
        alg: "none",
        typ: "JWT",
    };
    let payload = serde_json::json!({});
    let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
    let header_b64 = b64(&serde_json::to_vec(&header).unwrap());
    let payload_b64 = b64(&serde_json::to_vec(&payload).unwrap());
    let signature_b64 = b64(b"sig");
    let new_id = format!("{header_b64}.{payload_b64}.{signature_b64}");
    // Call update_tokens with a new id_token
    let _ = crate::auth_store::update_tokens(&auth_file, new_id.clone(), None, None).unwrap();

    // Read raw file and ensure id_token is still a string, equal to what we wrote
    let raw = std::fs::read_to_string(&auth_file).unwrap();
    let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(val["tokens"]["id_token"].as_str(), Some(new_id.as_str()));
}

#[test]
fn write_auth_json_is_python_compatible_shape() {
    let dir = tempdir().unwrap();
    let id_token = {
        #[derive(Serialize)]
        struct Header {
            alg: &'static str,
            typ: &'static str,
        }
        let header = Header {
            alg: "none",
            typ: "JWT",
        };
        let payload = serde_json::json!({"sub": "123"});
        let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
        let header_b64 = b64(&serde_json::to_vec(&header).unwrap());
        let payload_b64 = b64(&serde_json::to_vec(&payload).unwrap());
        let signature_b64 = b64(b"sig");
        format!("{header_b64}.{payload_b64}.{signature_b64}")
    };

    let tokens = crate::token_data::TokenData::from_raw(
        id_token.clone(),
        "a1".to_string(),
        "r1".to_string(),
        Some("acc".to_string()),
    )
    .unwrap();
    let auth = crate::auth_store::AuthDotJson {
        openai_api_key: Some("sk-test".to_string()),
        tokens: Some(tokens),
        last_refresh: Some(chrono::Utc::now()),
    };
    crate::auth_store::write_auth_json(&crate::auth_store::get_auth_file(dir.path()), &auth)
        .unwrap();

    let raw = std::fs::read_to_string(dir.path().join("auth.json")).unwrap();
    let val: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(val["OPENAI_API_KEY"].as_str(), Some("sk-test"));
    assert!(val["last_refresh"].as_str().is_some());
    assert!(val["tokens"].is_object());
    assert_eq!(val["tokens"]["id_token"].as_str(), Some(id_token.as_str()));
    assert_eq!(val["tokens"]["access_token"].as_str(), Some("a1"));
    assert_eq!(val["tokens"]["refresh_token"].as_str(), Some("r1"));
    assert_eq!(val["tokens"]["account_id"].as_str(), Some("acc"));
}
