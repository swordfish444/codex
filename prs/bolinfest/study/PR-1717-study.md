**DOs**
- Bold: Alphabetize Cargo dependencies: Keep `[dependencies]` sorted to reduce diff churn and ease review.
```toml
# Before (unsorted)
[dependencies]
open = "5"
url = "2"
hyper = { version = "1", features = ["http1", "server"] }
http-body-util = "0.1"
rand = "0.8"
base64 = "0.22"
sha2 = "0.10"
hyper-util = { version = "0.1", features = ["tokio"] }

# After (alphabetized)
[dependencies]
base64 = "0.22"
http-body-util = "0.1"
hyper = { version = "1", features = ["http1", "server"] }
hyper-util = { version = "0.1", features = ["tokio"] }
open = "5"
rand = "0.8"
sha2 = "0.10"
url = "2"
```

- Bold: Put public entrypoints first: List `pub` functions near the top; follow with helpers.
```rust
pub async fn run_login_server(codex_home: &Path) -> std::io::Result<()> {
    // ...
}

// helpers below
fn random_hex(len: usize) -> String { /* ... */ }
fn urlencode(params: &[(String, String)]) -> String { /* ... */ }
```

- Bold: Use clear names (avoid implementation-leaky names): Prefer `urlencode` over `serde_urlencode`.
```rust
fn urlencode(params: &[(String, String)]) -> String {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in params {
        s.append_pair(k, v);
    }
    s.finish()
}

// usage
let q = urlencode(&params);
let auth_url = format!("{issuer}/oauth/authorize?{q}");
```

- Bold: Propagate errors; avoid unwrap: Forward builder and I/O errors rather than calling `unwrap()`.
```rust
type HttpResult<T> = Result<T, (StatusCode, String)>;

fn build_ok_response(body: Bytes) -> HttpResult<Response<BodyFull<Bytes>>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(BodyFull::from(body))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Build response: {e}")))
}
```

- Bold: Preserve important comments from prior impls: Keep context like the required port note.
```rust
// Required port for OAuth client.
const REQUIRED_PORT: u16 = 1455;
```

- Bold: Centralize and name defaults consistently: Define `DEFAULT_CLIENT_ID` (or re-export) near other constants.
```rust
const DEFAULT_ISSUER: &str = "https://auth.openai.com";
const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

let client_id = DEFAULT_CLIENT_ID.to_string();
```

- Bold: Group constants at the top: Keep large includes and constants together for discoverability.
```rust
const URL_BASE: &str = "http://localhost:1455";
const LOGIN_SUCCESS_HTML: &str = include_str!("static/success.html");
```

- Bold: Keep deps minimal and scoped: Add only what you need; enable the smallest feature sets.
```toml
hyper = { version = "1", features = ["http1", "server"] }
hyper-util = { version = "0.1", features = ["tokio"] }
# Avoid higher-level frameworks unless strictly necessary.
```

- Bold: Log with inline formatting: Use inline `{var}` in `format!`/`eprintln!`.
```rust
if let Err(err) = open::that_detached(&auth_url) {
    eprintln!("Failed to open browser: {err}");
}
```

**DON’Ts**
- Bold: Don’t use unwrap in request paths: Avoid `.unwrap()` when building responses or parsing tokens.
```rust
// ❌
let resp = Response::builder().status(StatusCode::OK).body(body).unwrap();

// ✅
let resp = Response::builder()
    .status(StatusCode::OK)
    .body(body)
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Build response: {e}")))?;
```

- Bold: Don’t bury the public API after helpers: Reviewers expect the module’s main entrypoint up front.
```rust
// ❌ helpers first, pub fn far below

// ✅ pub first, helpers after (see DOs)
```

- Bold: Don’t use confusing helper names: Names like `serde_urlencode` imply serde involvement; prefer domain terms.
```rust
// ❌ fn serde_urlencode(...) -> String
// ✅ fn urlencode(...) -> String
```

- Bold: Don’t lose prior intentful comments: If the Python had a meaningful comment, carry it forward.
```rust
// Keep: // Required port for OAuth client.
```

- Bold: Don’t hide default IDs across modules: Avoid magic imports like `use crate::CLIENT_ID` if it obscures origin.
```rust
// ❌ use crate::CLIENT_ID; // unclear source, mismatched naming
// ✅ const DEFAULT_CLIENT_ID: &str = "..."; // or re-export with clear naming
```

- Bold: Don’t add heavy deps without justification: A full server stack has cost; keep dependency surface lean.
```toml
# ❌ Avoid adding broad web frameworks for a tiny callback handler.
# ✅ Use hyper + minimal utils only when necessary.
```