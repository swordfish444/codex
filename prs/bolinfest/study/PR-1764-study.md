**DOs**
- **Alpha-Sort Dependencies:** Keep Cargo.toml dependencies alphabetized for readability and easy diffs.
```toml
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["preserve_order"] }
```

- **Gate Update Logic:** Compile update-check code for release (and tests) only.
```rust
// at top of updates.rs
#![cfg(any(not(debug_assertions), test))]
```

- **Fetch Updates in Background:** Don’t block TUI startup; refresh cached version asynchronously.
```rust
let version_file = version_filepath(config);
tokio::spawn(async move {
    if let Err(e) = check_for_update(&version_file).await {
        tracing::error!("Failed to update version: {e}");
    }
});
```

- **Recommend Updates Safely:** Prefer npm when launched by npm; otherwise detect Homebrew install on macOS; else link to releases.
```rust
#[allow(clippy::print_stderr)]
#[cfg(not(debug_assertions))]
if let Some(latest) = updates::get_upgrade_version(&config) {
    let current = env!("CARGO_PKG_VERSION");
    let exe = std::env::current_exe()?;
    let managed_by_npm = std::env::var_os("CODEX_MANAGED_BY_NPM").is_some();

    eprintln!("{} {current} -> {latest}.", "✨⬆️ Update available!".bold().cyan());

    if managed_by_npm {
        eprintln!("Run {} to update.", "npm install -g @openai/codex@latest".cyan().on_black());
    } else if cfg!(target_os = "macos") && exe.starts_with("/opt/homebrew") {
        eprintln!("Run {} to update.", "brew upgrade codex".cyan().on_black());
    } else {
        eprintln!("See {} for the latest releases and installation options.",
            "https://github.com/openai/codex/releases/latest".cyan().on_black());
    }
    eprintln!("");
}
```

- **Join Paths Cleanly:** Build paths with `join`, not manual push/clone.
```rust
fn version_filepath(config: &Config) -> std::path::PathBuf {
    config.codex_home.join("version.json")
}
```

- **Use Top-Level Serde Types:** Define models once; derive Serialize/Deserialize; use chrono DateTime<Utc>.
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct VersionInfo {
    latest_version: String,
    last_checked_at: DateTime<Utc>,
}

#[derive(Deserialize, Debug, Clone)]
struct ReleaseInfo {
    tag_name: String,
}
```

- **Destructure JSON Results:** Pull out only what you need.
```rust
let ReleaseInfo { tag_name: latest_tag_name } = reqwest::Client::new()
    .get("https://api.github.com/repos/openai/codex/releases/latest")
    .header("User-Agent", format!("codex/{} (+https://github.com/openai/codex)", env!("CARGO_PKG_VERSION")))
    .send().await?
    .error_for_status()?
    .json::<ReleaseInfo>().await?;
```

- **Propagate Errors:** Don’t swallow filesystem errors; use `?`.
```rust
if let Some(parent) = version_file.parent() {
    tokio::fs::create_dir_all(parent).await?;
}
tokio::fs::write(version_file, format!("{}\n", serde_json::to_string(&info)?)).await?;
```

- **Inline Variables in format!:** Prefer inline braces for clarity and lint happiness.
```rust
return Err(anyhow::anyhow!("Failed to parse latest tag name '{latest_tag_name}'"));
```

- **Compare Strict Semver (no pre-release):** Parse major.minor.patch only; add tests for edge cases.
```rust
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().split('.');
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

fn is_newer(latest: &str, current: &str) -> Option<bool> {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => Some(l > c),
        _ => None,
    }
}

#[test]
fn prerelease_not_considered_newer() {
    assert_eq!(is_newer("0.11.0-beta.1", "0.11.0"), None);
}

#[test]
fn whitespace_ignored() {
    assert_eq!(is_newer(" 1.2.3 ", "1.2.2"), Some(true));
}
```

- **Send a User-Agent:** Include a helpful UA for GitHub API calls.
```rust
.header("User-Agent", format!("codex/{} (+https://github.com/openai/codex)", env!("CARGO_PKG_VERSION")))
```

**DON’Ts**
- **Don’t Block Startup on Network:** Avoid awaiting the HTTP call at boot.
```rust
// Avoid:
check_for_update(&version_file).await?; // blocks UI

// Prefer:
tokio::spawn(async move { let _ = check_for_update(&version_file).await; });
```

- **Don’t Rely on Fragile Path Heuristics:** Avoid assuming `/usr/local` means Homebrew.
```rust
// Avoid:
else if exe.starts_with("/usr/local") { /* assume Homebrew */ }

// Prefer a solid signal:
else if cfg!(target_os = "macos") && exe.starts_with("/opt/homebrew") { /* Homebrew */ }
// Or gate via a build-time flag set by the formula:
else if option_env!("CODEX_HOMEBREW_BUILD").is_some() { /* Homebrew */ }
```

- **Don’t Misname Functions:** Avoid names like `update_version` that imply self-updating the CLI.
```rust
// Avoid:
async fn update_version(...) -> Result<()> { /* writes metadata */ }

// Prefer:
async fn check_for_update(...) -> Result<()> { /* writes metadata */ }
```

- **Don’t Use .jsonl for Single Objects:** Save a single JSON object as `.json`, not `.jsonl`.
```rust
// Avoid:
const VERSION_FILENAME: &str = "version.jsonl";

// Prefer:
const VERSION_FILENAME: &str = "version.json";
```

- **Don’t Nest Serde Models in Functions:** Define them at module scope for reuse and clarity.
```rust
// Avoid:
async fn check_for_update(...) {
    #[derive(serde::Deserialize)]
    struct ReleaseInfo { tag_name: String }
    /* ... */
}

// Prefer: top-level `ReleaseInfo`
```

- **Don’t Swallow Errors with ok():** Propagate failures to logs/callers.
```rust
// Avoid:
tokio::fs::create_dir_all(parent).await.ok();
tokio::fs::write(version_file, json_line).await.ok();

// Prefer:
tokio::fs::create_dir_all(parent).await?;
tokio::fs::write(version_file, json_line).await?;
```

- **Don’t Treat Pre-Releases as Upgrades:** Ensure `0.11.0-beta.1` is not considered newer than `0.11.0`.
```rust
assert_eq!(is_newer("0.11.0-beta.1", "0.11.0"), None);
```

- **Don’t Hardcode Update Commands Blindly:** Recommend `npm` only when launched by npm; otherwise prefer strong signals or a link.
```rust
// Avoid:
eprintln!("Run npm install -g @openai/codex@latest");

// Prefer:
if std::env::var_os("CODEX_MANAGED_BY_NPM").is_some() { /* npm */ } else { /* brew or link */ }
```

- **Don’t Sprawl Update UI in lib.rs:** Keep the boot message thin; push logic into `updates.rs`.
```rust
// In lib.rs, keep it minimal:
if let Some(latest) = updates::get_upgrade_version(&config) { /* print short message */ }
```