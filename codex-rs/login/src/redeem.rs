use crate::auth_file::now_rfc3339_z;
use crate::jwt_utils::parse_jwt_claims;
use chrono::{DateTime, Duration, Utc};
use reqwest::blocking::Client;
use serde_json::json;
use std::time::Duration as StdDuration;

const DEFAULT_ISSUER: &str = "https://auth.openai.com";

pub(crate) fn maybe_redeem_credits(
    issuer: &str,
    client_id: &str,
    id_token_opt: Option<&str>,
    refresh_token: &str,
    codex_home: &std::path::Path,
) {
    let client = Client::builder()
        .timeout(StdDuration::from_secs(30))
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
                    // last_refresh is top-level
                    obj["last_refresh"] = serde_json::Value::String(now_rfc3339_z());
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

    // Subscription active > 7 days check (parity with Python script)
    if let Some(sub_start_str) = auth_claims
        .get("chatgpt_subscription_active_start")
        .and_then(|v| v.as_str())
    {
        if let Ok(sub_start) = DateTime::parse_from_rfc3339(sub_start_str)
            .map(|dt| dt.with_timezone(&Utc))
        {
            if Utc::now() - sub_start < Duration::days(7) {
                eprintln!(
                    "Sorry, your subscription must be active for more than 7 days to redeem credits."
                );
                return;
            }
        }
    }

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
    if let Ok(r) = resp {
        if let Ok(val) = r.json::<serde_json::Value>() {
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
    }
}


