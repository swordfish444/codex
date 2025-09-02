**DOs**
- **Gate Auth By Provider**: Require tokens only when `requires_auth` is true; keep others working.
```
let token_opt = if self.provider.requires_auth {
    let auth = self.auth.as_ref().ok_or_else(|| CodexErr::EnvVar(EnvVarError {
        var: "OPENAI_API_KEY".into(),
        instructions: Some("Create an API key (https://platform.openai.com) and export it.".into()),
    }))?;
    Some(auth.get_token().await?)
} else {
    None
};

let req = self.client
    .post(format!("{}/responses", base_url))
    .header("OpenAI-Beta", "responses=experimental")
    .header("session_id", self.session_id.to_string());

let req = if let Some(t) = token_opt { req.bearer_auth(t) } else { req };
let req = self.provider.apply_http_headers(req).json(&payload);
```

- **Preserve Provider Headers**: Always reapply provider headers after constructing custom requests.
```
let req = self.provider.apply_http_headers(req);
```

- **Normalize `base_url`**: Treat empty strings as `None`; choose sane defaults based on auth mode.
```
let base_url = self.provider.base_url
    .clone()
    .filter(|s| !s.trim().is_empty())
    .unwrap_or_else(|| match auth.mode {
        AuthMode::ChatGPT => "https://chatgpt.com/backend-api/codex".to_string(),
        AuthMode::ApiKey => "https://api.openai.com/v1".to_string(),
    });
```

- **Map `OPENAI_BASE_URL` To Option**: Filter out empty values when building the built-in provider.
```
base_url: std::env::var("OPENAI_BASE_URL").ok()
    .filter(|v| !v.trim().is_empty()),
```

- **Keep `Cargo.toml` Order Consistent**: Alphabetize and group external crates before workspace crates.
```toml
# External first (alphabetical)
bytes = "1.10.1"
chrono = { version = "0.4", features = ["serde"] }

# Workspace crates after (alphabetical)
codex-apply-patch = { path = "../apply-patch" }
codex-login = { path = "../login" }
```

- **Use File Locks For `auth.json`**: Coordinate across processes with advisory locks when reading/writing.
```rust
use fd_lock::RwLock;

let path = &auth_file;
let file = std::fs::OpenOptions::new().read(true).write(true).create(true).open(path)?;
let lock = RwLock::new(file);
let mut guard = lock.write()?;
// read -> modify -> write via guard.get_mut()
```

- **Make `PartialEq` Meaningful (Or Avoid It)**: Compare fields users expect to matter, not just mode.
```rust
impl PartialEq for CodexAuth {
    fn eq(&self, other: &Self) -> bool {
        self.mode == other.mode
            && self.api_key.is_some() == other.api_key.is_some()
            && self.auth_file == other.auth_file
    }
}
// Or: avoid PartialEq and compare explicitly in tests.
```

**DON’Ts**
- **Don’t Force Auth For All Providers**: Avoid unconditionally erroring when `self.auth` is `None`.
```
/* Bad */
let auth = self.auth.as_ref().ok_or_else(|| anyhow!("missing auth"))?;
```

- **Don’t Always Set `Authorization`**: Skip `bearer_auth` when no token is required.
```
/* Bad */
let req = req.bearer_auth(token); // token may be empty/irrelevant
```

- **Don’t Treat Empty `base_url` As Real**: Normalize `""`/whitespace to `None`.
```
/* Bad */
let base_url = Some(std::env::var("OPENAI_BASE_URL").unwrap_or_default()); // may be ""
```

- **Don’t Drop Provider Logic**: When crafting custom URLs, still apply provider headers and params.
```
/* Bad */
let req = client.post(url).json(&payload); // misses provider headers/params
```

- **Don’t Rely On `Mutex` For Cross-Process Safety**: In-process locks won’t protect shared files.
```
/* Bad */
let auth_dot_json = Arc::new(Mutex::new(Some(auth))); // races across processes
```