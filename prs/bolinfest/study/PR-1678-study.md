**DOs**

- Bold naming: accurate function names that reflect behavior.
  ```
  // Good: returns a rewritten argv, doesn't execute
  pub fn format_default_shell_invocation(&self, command: Vec<String>) -> Option<Vec<String>> { ... }
  ```

- Bold deps: keep Cargo.toml dependencies alphabetized.
  ```toml
  # Good
  [dependencies]
  anyhow = "1"
  reqwest = { version = "0.12", features = ["json", "stream"] }
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  sha1 = "0.10.6"
  shlex = "1.3.0"
  strum_macros = "0.27.2"
  thiserror = "2.0.12"
  time = { version = "0.3", features = ["formatting", "local-offset", "macros"] }
  uuid = { version = "1", features = ["serde", "v4"] }
  whoami = "1.6.0"
  wildmatch = "2.4.0"
  ```

- Bold detection: detect shell via $SHELL first, then fallback; use async.
  ```rust
  #[cfg(target_os = "macos")]
  pub async fn default_user_shell() -> Shell {
      if let Ok(shell_env) = std::env::var("SHELL") {
          if shell_env.ends_with("/zsh") {
              let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/Shared".into());
              return Shell::Zsh(ZshShell {
                  shell_path: shell_env,
                  zshrc_path: format!("{home}/.zshrc"),
              });
          }
      }
      let user = whoami::username();
      let home = format!("/Users/{user}");
      let out = tokio::process::Command::new("dscl")
          .args([".", "-read", &home, "UserShell"])
          .output()
          .await
          .ok();
      // parse as in PR…
      Shell::Unknown
  }
  ```

- Bold quoting: join argv safely with shlex.
  ```rust
  let joined = shlex::try_join(cmd.iter().map(|s| s.as_str()))?;
  let wrapper = format!("source {zshrc} && ({joined})");
  ```

- Bold robustness: handle missing profile gracefully.
  ```rust
  // Option A: skip if ~/.zshrc does not exist
  if !std::path::Path::new(&zsh.zshrc_path).exists() {
      return None;
  }

  // Option B: allow missing by ignoring source failure
  let wrapper = format!("source {zshrc} >/dev/null 2>&1; ({joined})");
  ```

- Bold isolation: use a subshell to avoid contaminating state.
  ```rust
  // (... ) runs the user command in a subshell
  result.push(format!("source {zshrc} && ({joined})"));
  ```

- Bold session: store a non-Option shell in Session with Unknown default.
  ```rust
  pub(crate) struct Session {
      // ...
      user_shell: shell::Shell, // Shell::Unknown by default
  }
  ```

- Bold ergonomics: shadow instead of cloning when possible.
  ```rust
  // Before
  // let processed = maybe_run_with_user_profile(params.clone(), sess);

  // After
  let params = maybe_run_with_user_profile(params, sess);
  ```

- Bold logging: add trace-level spawn diagnostics and env-driven filtering.
  ```rust
  // exec.rs
  trace!(
      "spawn_child_async: {program:?} {args:?} {arg0:?} {cwd:?} {sandbox_policy:?} {stdio_policy:?} {env:?}"
  );

  // mcp-server/src/lib.rs
  use tracing_subscriber::EnvFilter;
  tracing_subscriber::fmt()
      .with_writer(std::io::stderr)
      .with_env_filter(EnvFilter::from_default_env())
      .init();
  ```

- Bold opt-in: gate profile usage behind config flag → policy.
  ```rust
  // config_types.rs (toml → policy)
  pub struct ShellEnvironmentPolicyToml {
      pub experimental_use_profile: Option<bool>,
  }
  pub struct ShellEnvironmentPolicy {
      pub use_profile: bool,
  }
  impl From<ShellEnvironmentPolicyToml> for ShellEnvironmentPolicy {
      fn from(t: ShellEnvironmentPolicyToml) -> Self {
          Self { use_profile: t.experimental_use_profile.unwrap_or(false), /* ... */ }
      }
  }

  // codex.rs
  let params = maybe_run_with_user_profile(params, sess);
  ```

- Bold testing: make tests independent of user machine and HOME.
  ```rust
  #[tokio::test]
  async fn test_run_with_profile_escaping_and_execution() {
      let tmp = tempfile::tempdir().unwrap();
      let home = tmp.path().to_str().unwrap().to_string();
      std::env::set_var("HOME", &home);
      let zshrc = format!("{home}/.zshrc");
      std::fs::write(&zshrc, "function myecho { echo It\\ works!; }").unwrap();

      let shell = Shell::Zsh(ZshShell {
          shell_path: "/bin/zsh".into(),
          zshrc_path: zshrc.clone(),
      });

      let argv = shell.format_default_shell_invocation(vec!["myecho".into()]).unwrap();
      // run argv and assert output/exit code...
  }
  ```

- Bold validation: conditionally assert in env-dependent tests.
  ```rust
  #[tokio::test]
  async fn detects_zsh_if_current_shell_is_zsh() {
      let env_shell = std::env::var("SHELL").unwrap_or_default();
      if env_shell.ends_with("/zsh") {
          let home = std::env::var("HOME").unwrap();
          assert_eq!(
              default_user_shell().await,
              Shell::Zsh(ZshShell { shell_path: env_shell, zshrc_path: format!("{home}/.zshrc") })
          );
      }
  }
  ```

**DON’Ts**

- Bold blocking: don’t use std::process::Command for user-shell detection on macOS.
  ```rust
  // Bad: synchronous; can wedge CI
  let out = std::process::Command::new("dscl").args([...]).output().unwrap();
  ```

- Bold fragility: don’t assume ~/.zshrc exists or crash if it doesn’t.
  ```rust
  // Bad: will fail the whole command if missing or noisy
  result.push("source ~/.zshrc && (my cmd)".into());
  ```

- Bold leakage: don’t let profile output corrupt tool call stdout/stderr.
  ```rust
  // Bad: profile may print banners, `set -x`, etc.
  let wrapper = format!("source {zshrc} && ({joined})"); // no redirection
  ```

- Bold ambiguity: don’t name a formatter “run_*” if it doesn’t execute.
  ```rust
  // Bad
  pub fn run_with_profile(...) -> Option<Vec<String>> { ... }
  ```

- Bold over-engineering: don’t keep Option<Shell> in session; prefer Unknown.
  ```rust
  // Bad
  user_shell: Option<shell::Shell>,
  ```

- Bold waste: don’t clone args unnecessarily when shadowing suffices.
  ```rust
  // Bad
  let processed = maybe_run_with_user_profile(params.clone(), sess);
  ```

- Bold hardcoding: don’t construct home paths with brittle strings in tests/runtime.
  ```rust
  // Bad
  let home = format!("/Users/{user}");
  let zshrc = format!("{home}/.zshrc");
  ```

- Bold surprise: don’t make “use profile” implicit; require explicit opt-in.
  ```rust
  // Bad default
  ShellEnvironmentPolicy { use_profile: true, ..Default::default() }
  ```