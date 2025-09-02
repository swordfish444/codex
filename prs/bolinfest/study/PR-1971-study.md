**DOs**
- Bold the logic: Prefer ChatGPT auth for Free/Plus/Pro/Team plans even if an API key is present.
- Bold the fallback: Read `OPENAI_API_KEY` only when `auth.json` is missing (`NotFound`) and `include_env_var` is true.
- Bold the error handling: If `auth.json` exists but is malformed, return the error and do not fall back to env.
- Bold the API key choice: Use API key when `auth.json` has an API key and no tokens.
- Bold the enterprise routing: Use API key for Business/Enterprise/Edu or unknown plans.
- Bold the modeling: Represent plan with `PlanType` (`Known(KnownPlan)` or `Unknown(String)`) and use `serde` for `KnownPlan`.
- Bold the helper: Add `read_openai_api_key_from_env()` to encapsulate env access.
- Bold the ChatGPT shape: In ChatGPT mode, set `api_key: None` and keep only tokens/last_refresh; clear `openai_api_key`.
- Bold the display: Expose `IdTokenInfo::get_chatgpt_plan_type()` for UI and title-case it for display.
- Bold the tests: Use a minimal URL-safe, no-pad base64 JWT, a `LAST_REFRESH` constant, and `tempdir` helpers.

```rust
// Auth selection (core decision points)
if let Ok(auth) = try_read_auth_json(&auth_file) {
    let AuthDotJson { openai_api_key, tokens, last_refresh } = auth;

    if let Some(api_key) = &openai_api_key {
        match &tokens {
            Some(tokens) if tokens.is_plan_that_should_use_api_key() => {
                // Business/Enterprise/Edu/Unknown → API key
                return Ok(Some(CodexAuth::from_api_key(api_key)));
            }
            Some(_) => {
                // Free/Plus/Pro/Team → prefer ChatGPT
            }
            None => {
                // API key but no tokens → API key
                return Ok(Some(CodexAuth::from_api_key(api_key)));
            }
        }
    }

    // ChatGPT mode: no API key, keep tokens/last_refresh
    return Ok(Some(CodexAuth {
        api_key: None,
        mode: AuthMode::ChatGPT,
        auth_file,
        auth_dot_json: Arc::new(Mutex::new(Some(AuthDotJson {
            openai_api_key: None,
            tokens,
            last_refresh,
        }))),
    }));
} else if include_env_var && matches!(e.kind(), std::io::ErrorKind::NotFound) {
    // Only when auth.json is missing
    if let Some(k) = read_openai_api_key_from_env() {
        return Ok(Some(CodexAuth::from_api_key(&k)));
    }
    return Ok(None);
} else {
    // Malformed auth.json → surface error
    return Err(e);
}
```

```rust
// Plan modeling
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum PlanType {
    Known(KnownPlan),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum KnownPlan { Free, Plus, Pro, Team, Business, Enterprise, Edu }

impl PlanType {
    fn is_plan_that_should_use_api_key(&self) -> bool {
        match self {
            PlanType::Known(KnownPlan::Free | KnownPlan::Plus | KnownPlan::Pro | KnownPlan::Team) => false,
            PlanType::Known(_) | PlanType::Unknown(_) => true,
        }
    }
}

impl TokenData {
    pub(crate) fn is_plan_that_should_use_api_key(&self) -> bool {
        self.id_token.chatgpt_plan_type.as_ref().is_none_or(|p| p.is_plan_that_should_use_api_key())
    }
}
```

```rust
// Env helper
fn read_openai_api_key_from_env() -> Option<String> {
    env::var(OPENAI_API_KEY_ENV_VAR).ok().filter(|s| !s.is_empty())
}
```

```rust
// UI display
let plan_text = info
    .get_chatgpt_plan_type()
    .map(|s| title_case(&s))
    .unwrap_or_else(|| "Unknown".to_string());
lines.push(Line::from(vec!["  • Plan: ".into(), plan_text.into()]));

if let Some(email) = &info.email {
    lines.push(Line::from(vec!["  • Login: ".into(), email.clone().into()]));
}
```

```rust
// Test helper: write a minimal auth.json
let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
    serde_json::to_vec(&json!({ "alg": "none", "typ": "JWT" }))?,
);
let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
    serde_json::to_vec(&json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "user_id": "user-12345"
        }
    }))?,
);
let fake_jwt = format!("{header_b64}.{payload_b64}.{}", 
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"sig"));
```


**DON’Ts**
- Bold the wrong fallback: Do not fall back to `OPENAI_API_KEY` when `auth.json` exists but is malformed.
- Bold the plan mistake: Do not use API key for Free/Plus/Pro/Team when tokens indicate those plans.
- Bold the token ignore: Do not ignore tokens/plan when choosing between ChatGPT and API key.
- Bold the leakage: Do not carry `openai_api_key` forward in `auth_dot_json` when using ChatGPT mode; set it to `None`.
- Bold the ambiguity: Do not treat unknown plan strings as ChatGPT-eligible; unknown plans use API key.
- Bold the env misuse: Do not read `OPENAI_API_KEY` if `include_env_var` is false or `auth.json` is present.
- Bold the ownership slip: Do not move `email` out of `IdTokenInfo`; borrow (`&info.email`) and clone only when needed for ownership.
- Bold the string handling: Do not hand-roll plan strings; use `PlanType`/`KnownPlan` plus `get_chatgpt_plan_type()` and `title_case`.

```rust
// WRONG: falling back to env on malformed auth.json
match try_read_auth_json(&auth_file) {
    Err(_) if include_env_var => read_openai_api_key_from_env().map(...), // ✗ Don’t do this
    _ => {}
}

// WRONG: treating unknown plan as ChatGPT
match plan_type {
    PlanType::Unknown(_) => use_chatgpt(), // ✗ Should use API key
    _ => {}
}

// WRONG: keeping API key around in ChatGPT mode
CodexAuth { api_key: Some(api_key), mode: AuthMode::ChatGPT, .. } // ✗ api_key must be None
```