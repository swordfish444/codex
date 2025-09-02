**DOs**
- Bold: Atomic config writes: write `config.toml` via a temp file in `codex_home`, then atomically persist.
```rust
use tempfile::NamedTempFile;
use toml_edit::{DocumentMut, value};

const CONFIG_TOML_FILE: &str = "config.toml";

fn write_config_atomically(codex_home: &Path, doc: &DocumentMut) -> anyhow::Result<()> {
    let config_path = codex_home.join(CONFIG_TOML_FILE);
    std::fs::create_dir_all(codex_home)?;
    let tmp = NamedTempFile::new_in(codex_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}
```

- Bold: Autovivify with toml_edit: rely on table creation-on-demand when setting nested keys (no panics).
```rust
use toml_edit::{DocumentMut, value};

fn trust_project(doc: &mut DocumentMut, project: &Path) {
    let key = project.to_string_lossy();
    doc["projects"][key.as_ref()]["trust_level"] = value("trusted");
}
```

- Bold: Single source of cwd: canonicalize once, store in `Config`, and use `config.cwd` everywhere.
```rust
// During CLI parsing
let cwd = cli.cwd.clone().map(|p| p.canonicalize().unwrap_or(p));
let overrides = ConfigOverrides { cwd, /* ... */ };

// Later, prefer config.cwd over a separate arg
fn needs_trust_screen(config: &Config) -> bool {
    let resolved = &config.cwd;
    /* use resolved as the canonical cwd */
    /* ... */
    false
}
```

- Bold: Respect overrides and profiles: only show the trust screen when no explicit policy is set via CLI flags, `-c` overrides, profile, or `config.toml`.
```rust
fn determine_repo_trust_state(
    config: &mut Config,
    config_toml: &ConfigToml,
    approval_override: Option<AskForApproval>,
    sandbox_override: Option<SandboxMode>,
    profile_override: Option<String>,
) -> std::io::Result<bool> {
    let profile = config_toml.get_config_profile(profile_override)?;
    if approval_override.is_some() || sandbox_override.is_some() { return Ok(false); }
    if profile.approval_policy.is_some() { return Ok(false); }
    if config_toml.approval_policy.is_some() || config_toml.sandbox_mode.is_some() { return Ok(false); }
    if config_toml.is_cwd_trusted(&config.cwd) {
        config.approval_policy = AskForApproval::OnRequest;
        config.sandbox_policy = SandboxPolicy::new_workspace_write_policy();
        return Ok(false);
    }
    Ok(true)
}
```

- Bold: Apply `-c` overrides to `config.toml` for decisions: merge CLI key/value overrides before trust checks.
```rust
let cli_kv = cli.config_overrides.parse_overrides()?;
let codex_home = find_codex_home()?;
let config_toml = load_config_as_toml_with_cli_overrides(&codex_home, cli_kv)?;
```

- Bold: Keep “trust” UX in TUI: mutate session config in the trust step, not in `core`.
```rust
// In the TrustDirectory widget
if let Ok(mut args) = self.chat_widget_args.lock() {
    args.config.approval_policy = AskForApproval::OnRequest;
    args.config.sandbox_policy = SandboxPolicy::new_workspace_write_policy();
}
set_project_trusted(&self.codex_home, &self.cwd)?;
```

- Bold: Share mutable state only when needed: use `Arc<Mutex<...>>` to pass `ChatWidgetArgs` across steps that mutate it.
```rust
let shared = Arc::new(Mutex::new(chat_widget_args));
steps.push(Step::TrustDirectory(TrustDirectoryWidget { chat_widget_args: shared.clone(), /*...*/ }));
steps.push(Step::ContinueToChat(ContinueToChatWidget { chat_widget_args: shared }));
```

- Bold: Update docs when defaults change: if the default `AskForApproval` variant changes, annotate and update prose.
```rust
#[derive(Default, strum::Display, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskForApproval {
    OnFailure,
    #[default] // now the default
    OnRequest,
    UnlessTrusted,
}
/// Default ask-for-approval policy is OnRequest.
```


**DON’Ts**
- Bold: Non-atomic writes: don’t write `config.toml` directly or compute arbitrary parents; always write temp-then-persist in `codex_home`.
```rust
// ❌ Don’t
std::fs::write(codex_home.join("config.toml"), doc.to_string())?;
```

- Bold: Implicit trust from flags: don’t mark a project trusted just because user passed `--sandbox` or `--approval-policy`, and don’t persist that.
```rust
// ❌ Don’t persist trust due to transient flags
if approval_override.is_some() || sandbox_override.is_some() {
    // do not write projects[resolved_cwd].trust_level = "trusted"
}
```

- Bold: UI logic in core: don’t make `core` assume TUI onboarding; keep policy selection and trust prompts in TUI.
```rust
// ❌ Don’t in core
if cfg.projects.get(&cwd_str).map(|p| p.trusted == Some(true)).unwrap_or(false) {
    // core deciding UI flow — avoid
}
```

- Bold: Hardcoded paths: don’t sprinkle `"config.toml"` everywhere; use a constant.
```rust
// ✅ Do
const CONFIG_TOML_FILE: &str = "config.toml";
let p = codex_home.join(CONFIG_TOML_FILE);
```

- Bold: Ignoring profiles/overrides: don’t trigger trust onboarding if a profile or any override already sets policy/sandbox.
```rust
// ❌ Don’t
let show_trust = true; // ignoring profile/overrides present in config_toml or CLI
```

- Bold: Redundant cwd plumbing: don’t pass `cwd` separately when `config.cwd` is already resolved.
```rust
// ❌ Don’t
fn should_show_trust_screen(_config: &Config, cwd: Option<PathBuf>) -> bool { /* ... */ }
```

- Bold: Conflate Git with trust: don’t treat “inside a Git repo” as the trust signal or exit condition; use explicit trust state.
```rust
// ❌ Don’t
if !is_inside_git_repo(&config.cwd) { eprintln!("Exiting..."); std::process::exit(1); }
```

- Bold: Unnecessary locking: don’t wrap data in `Arc<Mutex<_>>` unless a later step will mutate it.
```rust
// ❌ Don’t
let args = Arc::new(Mutex::new(ChatWidgetArgs { /* ... */ })); // if never mutated, pass by value
```

- Bold: Vague errors: don’t lose detail; include variables directly in messages with captured `format!`.
```rust
// ✅ Do
return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("config profile `{key}` not found")));
```