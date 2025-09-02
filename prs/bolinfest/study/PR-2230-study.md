**DOs**
- **Centralize User-Agent construction**: Use a single helper to build a consistent UA string across all HTTP clients.
```rust
// core/src/user_agent.rs
const DEFAULT_ORIGINATOR: &str = "codex_cli_rs";

pub fn get_codex_user_agent(originator: Option<&str>) -> String {
    let build_version = env!("CARGO_PKG_VERSION");
    let os = os_info::get();
    format!(
        "{}/{build_version} ({} {}; {})",
        originator.unwrap_or(DEFAULT_ORIGINATOR),
        os.os_type(),
        os.version(),
        os.architecture().unwrap_or("unknown"),
    )
}
```

- **Set the UA on every request**: Pass the helper’s output into the "User-Agent" header instead of ad hoc strings.
```rust
let originator = Some("codex_cli_rs");
let ua = get_codex_user_agent(originator);

let resp = reqwest::Client::new()
    .get(url)
    .header("User-Agent", ua)
    .send()
    .await?;
```

- **Prefer regex-lite for tests**: Use `regex-lite` instead of `regex` to reduce size.
```toml
# core/Cargo.toml
[dev-dependencies]
regex-lite = "0.1.6"
```
```rust
use regex_lite::Regex;

let re = Regex::new(r"^codex_cli_rs/\d+\.\d+\.\d+ \(.+; (x86_64|arm64|aarch64)\)$").unwrap();
assert!(re.is_match(&get_codex_user_agent(None)));
```

- **Add OS-specific UA tests**: Validate platform formatting with `#[cfg(target_os = "...")]`.
```rust
#[cfg(target_os = "macos")]
#[test]
fn ua_macos() {
    use regex_lite::Regex;
    let re = Regex::new(r"^codex_cli_rs/\d+\.\d+\.\d+ \(Mac OS \d+(\.\d+)*; (x86_64|arm64)\)$").unwrap();
    assert!(re.is_match(&get_codex_user_agent(None)));
}

#[cfg(target_os = "windows")]
#[test]
fn ua_windows() {
    use regex_lite::Regex;
    let re = Regex::new(r"^codex_cli_rs/\d+\.\d+\.\d+ \(Windows \d+(\.\d+)*; (x86_64|arm64)\)$").unwrap();
    assert!(re.is_match(&get_codex_user_agent(None)));
}

#[cfg(target_os = "linux")]
#[test]
fn ua_linux() {
    use regex_lite::Regex;
    let re = Regex::new(r"^codex_cli_rs/\d+\.\d+\.\d+ \(Linux .+; (x86_64|aarch64)\)$").unwrap();
    assert!(re.is_match(&get_codex_user_agent(None)));
}
```

- **Extract version once (follow-up)**: Consider a `version.rs` to avoid repeating `env!("CARGO_PKG_VERSION")`.
```rust
// core/src/version.rs
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// core/src/user_agent.rs
use crate::version::VERSION;
pub fn get_codex_user_agent(originator: Option<&str>) -> String {
    let os = os_info::get();
    format!(
        "{}/{VERSION} ({} {}; {})",
        originator.unwrap_or(DEFAULT_ORIGINATOR),
        os.os_type(),
        os.version(),
        os.architecture().unwrap_or("unknown"),
    )
}
```

- **Use format! with inline vars**: Inline variables directly in `{}` to keep strings tidy.
```rust
let version = "1.2.3";
let name = "codex_cli_rs";
let s = format!("{name}/{version}");
```


**DON’Ts**
- **Don’t hardcode UA strings**: Avoid per-call literals that drift from the canonical format.
```rust
// Bad
.header("User-Agent", "codex-cli")

// Good
.header("User-Agent", get_codex_user_agent(None))
```

- **Don’t duplicate the default originator literal**: Use a single `const` instead of repeating `"codex_cli_rs"`.
```rust
// Bad
originator.unwrap_or("codex_cli_rs")

// Good
originator.unwrap_or(DEFAULT_ORIGINATOR)
```

- **Don’t depend on heavy `regex` for simple tests**: Prefer `regex-lite` unless you truly need advanced features.
```toml
# Bad
regex = "1.10"

// Good
regex-lite = "0.1.6"
```

- **Don’t scatter `env!(...)` across modules**: Centralize version access instead of inlining everywhere.
```rust
// Bad (repeated)
let v = env!("CARGO_PKG_VERSION");

// Good
use crate::version::VERSION;
let v = VERSION;
```

- **Don’t ignore available originator context**: Pass it through when you have it.
```rust
// Bad
.header("User-Agent", get_codex_user_agent(None))

// Good
let originator = Some("codex_cli_rs");
.header("User-Agent", get_codex_user_agent(originator))
```

- **Don’t build strings with concatenation when format! fits**: Keep string assembly consistent and readable.
```rust
// Bad
let s = "codex/".to_string() + version + " (" + os + ")";

// Good
let s = format!("codex/{version} ({os})");
```