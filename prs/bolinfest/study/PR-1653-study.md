**Dotenv Support: Practical Guide (PR #1653)**

**DOs**
- **Load Early:** Call `load_dotenv()` before creating any threads or the Tokio runtime.
  ```rust
  // binary/lib entry point
  codex_common::dotenv::load_dotenv();
  let rt = tokio::runtime::Runtime::new()?;
  rt.block_on(async_main());
  ```
- **Support Both Locations:** Load from `~/.codex/.env` (via `CODEX_HOME`) and the current directory’s `.env`.
  ```rust
  pub fn load_dotenv() {
      if let Ok(codex_home) = codex_core::config::find_codex_home() {
          dotenvy::from_path(codex_home.join(".env")).ok();
      }
      dotenvy::dotenv().ok();
  }
  ```
- **Reuse Core Logic:** Use `codex_core::config::find_codex_home()` (now `pub`) instead of duplicating path logic.
  ```rust
  let env_path = codex_core::config::find_codex_home()?.join(".env");
  dotenvy::from_path(env_path).ok();
  ```
- **Feature-Gate for CLI:** Expose the helper only when the `cli` feature is enabled and depend on it from CLI-facing crates.
  ```toml
  # codex-rs/linux-sandbox/Cargo.toml
  [dependencies]
  codex-common = { path = "../common", features = ["cli"] }
  dotenvy = "0.15.7"
  ```
- **Fail Softly:** Ignore missing files and parse errors; never panic on dotenv load.
  ```rust
  dotenvy::from_path(some_path).ok(); // ignore errors intentionally
  dotenvy::dotenv().ok();
  ```
- **Keep Side Effects Minimal:** Only modify the process environment—no logging or user-facing output when loading.
  ```rust
  // no println!/tracing during dotenv load
  ```

**DON’Ts**
- **Don’t Load Late:** Avoid calling `load_dotenv()` after starting the runtime or in spawned tasks.
  ```rust
  // ❌ Bad: too late; threads already exist
  let rt = tokio::runtime::Runtime::new()?;
  rt.block_on(async {
      codex_common::dotenv::load_dotenv(); // too late
  });
  ```
- **Don’t Unwrap on IO:** Never `unwrap()`/`expect()` dotenv results; keep startup resilient.
  ```rust
  // ❌ Bad
  dotenvy::dotenv().expect("must have .env");
  // ✅ Good
  dotenvy::dotenv().ok();
  ```
- **Don’t Reimplement Home Discovery:** Don’t hardcode `~/.codex`; rely on `find_codex_home()`.
  ```rust
  // ❌ Bad
  let path = dirs::home_dir().unwrap().join(".codex/.env");
  // ✅ Good
  let path = codex_core::config::find_codex_home()?.join(".env");
  ```
- **Don’t Leak Secrets:** Don’t print or log env values loaded from `.env`.
  ```rust
  // ❌ Bad
  println!("{}", std::env::var("OPENAI_API_KEY").unwrap());
  ```
- **Don’t Make It Global by Default:** Don’t include dotenv in non-CLI builds; keep it behind the `cli` feature.
  ```toml
  # ❌ Bad: unconditional dependency
  codex-common = { path = "../common" }
  # ✅ Good
  codex-common = { path = "../common", features = ["cli"] }
  ```
- **Don’t Treat Missing CODEX_HOME as Error:** If `CODEX_HOME` isn’t set or the file isn’t present, proceed without failing.
  ```rust
  if let Ok(home) = codex_core::config::find_codex_home() {
      dotenvy::from_path(home.join(".env")).ok();
  } // else: do nothing
  ```