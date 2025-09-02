**DOs**
- Use clear boolean logic for prompts: prefer De Morgan instead of a double-negative.
  ```rust
  // Good: explicit and readable
  if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("y") {
      std::process::exit(1);
  }
  ```

- Propagate async at boundaries and update all call sites.
  ```rust
  // lib
  pub async fn run_main(cli: Cli, exe: Option<PathBuf>) -> std::io::Result<TokenUsage> { /* ... */ }

  // callers
  let usage = codex_tui::run_main(tui_cli, codex_linux_sandbox_exe).await?;
  ```

- Replace oneshot + spawn + blocking_recv with a direct await when no concurrency is needed.
  ```rust
  // Good: await directly
  match try_read_openai_api_key(&codex_home).await {
      Ok(key) => { set_openai_api_key(key); return Ok(()); }
      Err(_e) => { /* fall back to login */ }
  }
  ```

- Bound long operations with tokio::time::timeout and convert failures to errors, not panics.
  ```rust
  use std::time::Duration;
  use tokio::time::timeout;

  let refreshed = timeout(Duration::from_secs(60), try_refresh_token(&auth))
      .await
      .map_err(|_| std::io::Error::other("timed out while refreshing OpenAI API key"))??;
  ```

- Flush prompts before reading stdin so the user sees them.
  ```rust
  use std::io::{Write, Read};

  std::io::stdout().write_all(b"May I open a browser? [Yn] ")?;
  std::io::stdout().flush()?;
  let mut input = String::new();
  std::io::stdin().read_line(&mut input)?;
  ```

- Set the in-memory API key immediately after successful login/refresh.
  ```rust
  let new_key = codex_login::login_with_chatgpt(&config.codex_home, false).await?;
  set_openai_api_key(new_key);
  ```

- Remove obsolete Clippy allowances once they’re not needed.
  ```rust
  // Do: delete unnecessary allows when expect/unwrap/printing are gone
  // #[allow(clippy::expect_used)]
  // #[allow(clippy::print_stderr)]
  ```

**DON’Ts**
- Don’t panic on timeouts or rely on expect for recoverable paths.
  ```rust
  // Bad: panics on timeout
  tokio::time::timeout(Duration::from_secs(60), try_read_openai_api_key(&home))
      .await
      .expect("timed out");
  ```

- Don’t keep oneshot channels when you aren’t spawning a task.
  ```rust
  // Bad: unnecessary indirection
  let (tx, rx) = tokio::sync::oneshot::channel();
  tokio::spawn(async move {
      let _ = tx.send(try_read_openai_api_key(&home).await.is_err());
  });
  rx.await.unwrap();
  ```

- Don’t use blocking_recv or block_in_place to bridge async unless truly required.
  ```rust
  // Bad: mixes blocking with async
  tokio::task::block_in_place(|| rx.blocking_recv()).unwrap();
  ```

- Don’t leave double-negatives that obscure intent.
  ```rust
  // Bad: harder to parse
  if !(trimmed.is_empty() || trimmed.eq_ignore_ascii_case("y")) {
      std::process::exit(1);
  }
  ```

- Don’t carry forward unused Clippy allows or module/state leftovers after refactors.
  ```rust
  // Bad: stale allow and unused variant
  #[allow(clippy::unwrap_used)]
  enum AppState { Chat, /* Login, */ GitWarning }
  ```