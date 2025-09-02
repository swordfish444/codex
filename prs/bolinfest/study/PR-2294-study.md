**DOs**

- Reuse library logic: Keep CLI thin by delegating to `codex-login` crate helpers instead of duplicating flow setup and URL printing.
  ```rust
  // cli/src/login.rs
  pub async fn login_with_chatgpt(codex_home: &Path) -> std::io::Result<()> {
      let (tx, rx) = std::sync::mpsc::channel();
      let opts = codex_login::ServerOptions::new(codex_home, codex_login::CLIENT_ID);

      let url_printer = std::thread::spawn(move || {
          if let Ok(info) = rx.recv() {
              eprintln!(
                  "Starting local login server on http://localhost:{}.\nIf your browser did not open, navigate to this URL to authenticate:\n\n{}",
                  info.actual_port, info.auth_url
              );
          }
      });

      tokio::task::spawn_blocking(move || codex_login::run_server_blocking_with_notify(opts, Some(tx), None))
          .await
          .map_err(std::io::Error::other)??;

      let _ = url_printer.join();
      Ok(())
  }
  ```

- Keep docs in sync with code: Update docstrings when implementations change (e.g., from subprocess to in-proc server with cancel).
  ```rust
  /// Represents a running login server.
  /// - `get_login_url()` returns Some(url) after the server starts.
  /// - `get_auth_result()` returns Some(true|false) when the flow completes.
  /// - Call `cancel()` to request shutdown.
  #[derive(Debug, Clone)]
  pub struct SpawnedLogin { /* ... */ }
  ```

- Use clear method names with obvious semantics: Prefer `get_auth_result()` over ambiguous names like `try_status()`.
  ```rust
  impl SpawnedLogin {
      pub fn get_auth_result(&self) -> Option<bool> { /* ... */ }
  }
  ```

- Document usage patterns for async/polling APIs: Explain when `get_login_url()` and `get_auth_result()` return `Some`.
  ```rust
  // Example usage
  let login = codex_login::spawn_login_with_chatgpt(&codex_home)?;
  while login.get_login_url().is_none() {
      std::thread::sleep(std::time::Duration::from_millis(50));
  }
  eprintln!("Open {}", login.get_login_url().unwrap());

  while login.get_auth_result().is_none() {
      std::thread::sleep(std::time::Duration::from_millis(100));
  }
  ```

- Prefer robust signaling primitives over ad-hoc atomics when appropriate: Consider `tokio::sync::Notify` or channels for shutdown/ready notifications.
  ```rust
  // Using Notify for shutdown
  let notify = std::sync::Arc::new(tokio::sync::Notify::new());
  let n = notify.clone();
  tokio::spawn(async move {
      // ... run server loop ...
      n.notified().await; // shutdown signal
  });

  // elsewhere
  notify.notify_one(); // request shutdown
  ```

- Remove unnecessary `drop(...)` calls: Let values go out of scope or use `JoinHandle::join()` when needed.
  ```rust
  // Bad
  drop(url_printer);

  // Good
  let _ = url_printer.join();
  ```

- Keep assets tidy: Strip trailing/extra blank lines and whitespace in HTML and other assets.
  ```html
  <!-- login/src/assets/success.html -->
  <!DOCTYPE html>
  <html lang="en">
  <!-- ...no trailing blank lines at EOF... -->
  </html>
  ```

- Ensure binaries are declared or removed: If adding a `src/bin/*.rs`, add a `[bin]` entry or rely on cargo’s auto-discovery.
  ```toml
  # login/Cargo.toml
  [[bin]]
  name = "codex-login-server"
  path = "src/bin/codex-login-server.rs"
  ```

- Explain function contracts near their definitions: Place entry points like `login_with_chatgpt` near the top and state their guarantees.
  ```rust
  /// Runs the browser-based login flow.
  /// Returns Ok(()) after persisting tokens to `codex_home/auth.json`.
  pub async fn login_with_chatgpt(codex_home: &Path) -> std::io::Result<()> { /* ... */ }
  ```

- Guard network-dependent tests in sandboxed CI: Skip when network is disabled.
  ```rust
  // login/tests/login_server_e2e.rs
  pub const CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR: &str = "CODEX_SANDBOX_NETWORK_DISABLED";

  #[test]
  fn end_to_end_login_flow_persists_auth_json() {
      if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
          println!("Skipping test because it cannot execute when network is disabled in a Codex sandbox.");
          return;
      }
      // ... test body ...
  }
  ```

- Inline variables in format strings: Prefer the concise `{var}` form.
  ```rust
  let auth_url = format!("{issuer}/oauth/authorize?{qs}");
  let msg = format!("Server running on http://localhost:{actual_port}");
  ```

**DON’Ts**

- Don’t duplicate flow logic across crates: Avoid re-implementing URL construction, PKCE, or server loops in the CLI.
  ```rust
  // Don’t: copy-paste logic from codex-login into the CLI
  // Do: call codex_login::run_server_blocking_with_notify(...)
  ```

- Don’t leave stale docs after refactors: Remove references to “child process kill()” when the implementation uses an in-process server with `cancel()`.

- Don’t use ambiguous method names or undocumented polling: Names like `try_status()` force readers to inspect internals.
  ```rust
  // Don’t
  fn try_status(&self) -> Option<bool>;

  // Do
  fn get_auth_result(&self) -> Option<bool>;
  ```

- Don’t rely on explicit `drop(...)` to manage lifetimes: Use RAII and explicit `join()` for threads/tasks instead of `drop(handle)`.

- Don’t add `src/bin/*.rs` without wiring it up: If the binary isn’t meant to ship, remove it; otherwise, declare it in `Cargo.toml`.

- Don’t assume tests can always reach the network: Missing guards cause flaky CI in sandboxed environments.

- Don’t bury the primary entry point: Avoid hiding `login_with_chatgpt` mid-file without a doc that states “Ok means auth.json is written”.

- Don’t leave trailing whitespace/blank lines in assets: Keep HTML/CSS/JS clean to reduce diff noise.

- Don’t overuse atomics for coordination when higher-level primitives fit better: Favor `Notify`/channels when they clarify intent and lifecycle.