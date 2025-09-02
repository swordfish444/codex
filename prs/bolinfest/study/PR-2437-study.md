**DOs**
- **Sanitize Header Tokens**: Clean any env-derived token before adding it to headers.
```rust
fn is_valid_header_value_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/')
}

fn sanitize_header_value(value: &str) -> String {
    value.chars().map(|c| if is_valid_header_value_char(c) { c } else { '_' }).collect()
}

// Usage
let token = sanitize_header_value(&format!("WezTerm/{v}"));
```
- **Use Concise Return-if/else**: Prefer a single expression to avoid duplicate returns.
```rust
return if !v.trim().is_empty() {
    format!("WezTerm/{v}")
} else {
    "WezTerm".to_string()
};
```
- **Validate Env Vars**: Check presence and non-emptiness before use.
```rust
if let Ok(tp) = std::env::var("TERM_PROGRAM") && !tp.trim().is_empty() {
    let ver = std::env::var("TERM_PROGRAM_VERSION").ok();
    return match ver {
        Some(v) if !v.trim().is_empty() => format!("{tp}/{v}"),
        _ => tp,
    };
}
```
- **Cache Detection Once**: Compute terminal UA once with `OnceLock`.
```rust
use std::sync::OnceLock;

static TERMINAL: OnceLock<String> = OnceLock::new();

pub fn user_agent() -> String {
    TERMINAL.get_or_init(detect_terminal).to_string()
}
```
- **Append Terminal to UA String**: Extend the UA format and keep variables inline.
```rust
pub fn get_codex_user_agent(originator: Option<&str>) -> String {
    let build_version = env!("CARGO_PKG_VERSION");
    let os_info = os_info::get();
    format!(
        "{}/{build_version} ({} {}; {}) {}",
        originator.unwrap_or("codex_cli_rs"),
        os_info.os_type(),
        os_info.version(),
        os_info.architecture().unwrap_or("unknown"),
        crate::terminal::user_agent()
    )
}
```
- **Update Tests for New Shape**: Adjust regex to include the terminal token.
```rust
use regex_lite::Regex;

let re = Regex::new(
    r"^codex_cli_rs/\d+\.\d+\.\d+ \(Mac OS \d+\.\d+\.\d+; (x86_64|arm64)\) (\S+)$"
).unwrap();
assert!(re.is_match(&get_codex_user_agent(None)));
```
- **Provide Clear Fallbacks**: Detect common terminals; fall back to `TERM` or `unknown`.
```rust
fn detect_terminal() -> String {
    // … TERM_PROGRAM branch …
    if let Ok(v) = std::env::var("WEZTERM_VERSION") {
        return if !v.trim().is_empty() { format!("WezTerm/{v}") } else { "WezTerm".to_string() };
    } else if std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM").map(|t| t.contains("kitty")).unwrap_or(false) {
        return "kitty".to_string();
    } else if std::env::var("ALACRITTY_SOCKET").is_ok()
        || std::env::var("TERM").map(|t| t == "alacritty").unwrap_or(false) {
        return "Alacritty".to_string();
    } else if let Ok(v) = std::env::var("KONSOLE_VERSION") {
        return if !v.trim().is_empty() { format!("Konsole/{v}") } else { "Konsole".to_string() };
    } else if std::env::var("GNOME_TERMINAL_SCREEN").is_ok() {
        return "gnome-terminal".to_string();
    } else if let Ok(v) = std::env::var("VTE_VERSION") {
        return if !v.trim().is_empty() { format!("VTE/{v}") } else { "VTE".to_string() };
    } else if std::env::var("WT_SESSION").is_ok() {
        return "WindowsTerminal".to_string();
    }
    std::env::var("TERM").unwrap_or_else(|_| "unknown".to_string())
}
```

**DON’Ts**
- **Don’t Trust Env Strings Blindly**: Never shove raw env values into headers.
```rust
// ❌ Bad
let token = format!("WezTerm/{}", std::env::var("WEZTERM_VERSION").unwrap());

// ✅ Good
let token = sanitize_header_value(&format!("WezTerm/{}", std::env::var("WEZTERM_VERSION").unwrap_or_default()));
```
- **Don’t Duplicate Control Flow**: Avoid multiple early returns that repeat logic.
```rust
// ❌ Bad
if !v.trim().is_empty() { return format!("WezTerm/{v}"); }
return "WezTerm".to_string();

// ✅ Good
return if !v.trim().is_empty() { format!("WezTerm/{v}") } else { "WezTerm".to_string() };
```
- **Don’t Assume Presence ⇒ Validity**: An env var can exist but be empty or whitespace.
```rust
// ❌ Bad
if let Ok(tp) = std::env::var("TERM_PROGRAM") {
    return tp; // might be empty/whitespace
}
```
- **Don’t Change UA Shape Without Tests**: Keep snapshot/regex tests in sync with header changes.
```rust
// ❌ Old pattern (missing terminal token)
let re = Regex::new(r"^codex_cli_rs/\d+\.\d+\.\d+ \(Mac OS \d+\.\d+\.\d+; (x86_64|arm64)\)$").unwrap();
```