**DOs**
- **Use enums for strategy flags**: Replace ambiguous booleans with domain enums to make call sites self-documenting and extensible.
```rust
// Before
pub fn from_codex_home(path: &Path, always_use_api_key_signing: bool) -> io::Result<Option<CodexAuth>> { /* ... */ }

// After
#[derive(Clone, Copy, Debug)]
pub enum AuthCredentialStrategy {
    ApiKeyOnly,
    PreferChatGpt,
}

pub fn from_codex_home(path: &Path, strategy: AuthCredentialStrategy) -> io::Result<Option<CodexAuth>> { /* ... */ }

// Call site
let auth = from_codex_home(&codex_home, AuthCredentialStrategy::PreferChatGpt)?;
```

- **Name enum variants precisely**: Prefer explicit status names like `NotAuthenticated` instead of vague `None`.
```rust
// Before
pub enum LoginStatus {
    ChatGPT,
    ApiKey,
    None,
}

// After
pub enum LoginStatus {
    AuthMode(AuthMode),  // AuthMode::{ChatGPT, ApiKey}
    NotAuthenticated,
}
```

- **Prefer exhaustive `match` over `matches!()`**: Use `match` when logic should evolve if new variants are added; it forces handling at compile time.
```rust
// Before
let show_login = matches!(status, LoginStatus::NotAuthenticated);

// After (exhaustive and future-proof)
let show_login = match status {
    LoginStatus::NotAuthenticated => true,
    LoginStatus::AuthMode(_) => false,
};
```

- **Compose enums instead of embedding booleans**: If a variant needs extra detail, model it with another enum, not a `bool`.
```rust
// Before
pub enum LoginStatus {
    ApiKey { always_use_api_key_signing: bool },
    AuthMode(AuthMode),
    NotAuthenticated,
}

// After
pub enum ApiKeyStrategy { Always, Optional }

pub enum LoginStatus {
    ApiKey { strategy: ApiKeyStrategy },
    AuthMode(AuthMode),
    NotAuthenticated,
}
```

**DON’Ts**
- **Don’t pass raw booleans into public APIs**: Avoid parameters like `always_use_api_key_signing: bool`; they obscure intent and don’t scale.
- **Don’t use vague variant names**: Names like `None` hide meaning; prefer `NotAuthenticated` (or similarly descriptive).
- **Don’t rely on `matches!()` for evolving control flow**: It won’t force updates when new enum variants are introduced.
- **Don’t hide decisions as bool fields inside enum payloads**: Replace embedded flags with well-named enums to clarify behavior and enable exhaustive handling.