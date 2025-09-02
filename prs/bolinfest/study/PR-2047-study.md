**DOs**
- **Keep PRs Single‑Purpose:** Split mechanical refactors (file moves, moduleization) from behavior changes to ease review.
  ```
  # Commit 1: purely mechanical split (no logic changes)
  git mv codex-rs/login/src/lib.rs codex-rs/login/src/auth.rs
  git add -A
  git commit -m "login: split lib.rs into modules (no functional changes)"

  # Commit 2: implement new behavior
  git add -A
  git commit -m "login: port login server to Rust"
  ```
- **Alphabetize `Cargo.toml` Dependencies:** Maintain consistent, sorted entries to reduce diff noise.
  ```toml
  # Bad (unsorted)
  [dependencies]
  tokio = { version = "1", features = ["macros"] }
  html-escape = "0.2"
  hex = "0.4"

  # Good (sorted)
  [dependencies]
  hex = "0.4"
  html-escape = "0.2"
  tokio = { version = "1", features = ["macros"] }
  ```
- **Preserve Helpful Comments:** Keep concise docs that carry intent, tradeoffs, or edge‑case rationale.
  ```rust
  // Good: explains when API-key mode is preferred and why.
  /// Returns true if the subscription plan should use metered API-key billing.
  pub(crate) fn is_plan_that_should_use_api_key(&self) -> bool { /* ... */ }
  ```
- **Remove Stray/Placeholder Code:** Delete dead lines like solitary `//` or commented stubs before posting.
  ```diff
  - //
  ```
- **Gate Features End‑to‑End (When You Add Them):** If introducing a feature flag, ensure both code and deps are actually optional.
  ```toml
  # Cargo.toml
  [features]
  login-server = []        # default off

  [dependencies]
  tiny_http  = { version = "0.12", optional = true }
  webbrowser = { version = "1",    optional = true }

  [features]
  default = []             # server not built by default
  ```

  ```rust
  // lib.rs
  #[cfg(feature = "login-server")]
  mod server;
  #[cfg(feature = "login-server")]
  pub use server::run_local_login_server_with_options;

  #[cfg(not(feature = "login-server"))]
  pub fn run_local_login_server_with_options(_: LoginServerOptions) -> std::io::Result<()> {
      Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "login-server feature disabled"))
  }
  ```
- **Prefer Not Bundling a Webserver by Default:** If a local HTTP server is necessary, isolate it behind a feature or separate crate; otherwise, just open the auth URL and guide the user.
  ```rust
  pub fn begin_login(url: &str) {
      eprintln!("If the browser does not open, open this URL:\n\n{url}");
      let _ = webbrowser::open(url);
  }
  ```

**DON’Ts**
- **Don’t Mix Refactor and Port in One PR:** Avoid combining file‑splits with logic changes; it obscures signal for reviewers.
  ```
  # Bad: single PR with large moves + new behavior intertwined
  # Hard to diff; hard to bisect regressions
  ```
- **Don’t Add Feature Flags Without Real Gating:** A feature that doesn’t exclude any code or dependencies only multiplies build permutations.
  ```toml
  # Bad: adds a feature but leaves server code and deps unconditional
  [features]
  http-e2e-tests = []

  [dependencies]
  tiny_http = "0.12"  # not optional → always compiled
  ```
- **Don’t Leave Placeholder/Noise:** Remove `//`, commented‑out blocks, or “delete?” markers before review.
  ```diff
  - // TODO: delete?
  ```
- **Don’t Drop Useful Docs:** Removing clarifying comments (e.g., around token/plan logic) increases re‑learning cost for future changes.
  ```diff
  - /// Returns true if this is a plan that should use the traditional
  - /// "metered" billing via an API key.
    pub(crate) fn is_plan_that_should_use_api_key(&self) -> bool { /* ... */ }
  ```
- **Don’t Leave `Cargo.toml` Unsorted:** Unsorted deps create noisy diffs and invite merge churn.
  ```toml
  # Bad
  [dependencies]
  tokio = "1"
  base64 = "0.22"
  ascii = "1.1"

  # Good (alphabetized)
  [dependencies]
  ascii  = "1.1"
  base64 = "0.22"
  tokio  = "1"
  ```
- **Don’t Bundle a Webserver Unnecessarily:** Shipping a tiny HTTP server in the CLI by default increases surface area and maintenance. If you must include it, hide it behind a feature that is truly optional (code and deps).