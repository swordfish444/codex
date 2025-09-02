**DOs**
- Bold the keyword: prefer callsite timeouts.
  ```rust
  // Option A: pass a budget into the collector
  use std::time::Duration;
  use tokio::time::timeout;

  let cwd = sess.as_ref().unwrap().cwd.clone();
  let overall = Duration::from_millis(500);
  let per_call = overall / 2;

  let git_info = timeout(overall, collect_git_info_with_timeout(&cwd, per_call))
      .await
      .ok()  // timeout elapsed
      .flatten(); // Option<GitInfo>
  ```

- Bold the keyword: tailor budgets to context.
  ```rust
  // Latency-sensitive handshake (short)
  let git_budget = Duration::from_millis(400);

  // Less latency-sensitive rollout (longer)
  let git_budget = Duration::from_secs(2);
  ```

- Bold the keyword: run independent work concurrently with time bounds.
  ```rust
  use tokio::time::{timeout, Duration};

  let (git_info, (history_log_id, history_entry_count)) = tokio::join!(
      async {
          timeout(Duration::from_millis(400),
                  collect_git_info_with_timeout(&cwd, Duration::from_millis(200)))
              .await
              .ok()
              .flatten()
      },
      async {
          timeout(Duration::from_millis(400), message_history::history_metadata(&config))
              .await
              .unwrap_or(Ok((0, 0))) // choose a safe fallback for handshake
              .unwrap_or((0, 0))
      }
  );
  ```

- Bold the keyword: surface branch via SessionConfiguredEvent (avoid sync in TUI).
  ```rust
  // In TUI, consume event.git_info rather than calling blocking helpers
  let branch_suffix = event.git_info.as_ref()
      .and_then(|g| g.branch.as_ref())
      .map(|b| format!(" ({b})"))
      .unwrap_or_default();

  let path_and_branch = if branch_suffix.is_empty() {
      format!(" {cwd_str}")
  } else {
      format!(" {cwd_str}{branch_suffix}")
  };
  ```

- Bold the keyword: use Stylize helpers and format! interpolation.
  ```rust
  use ratatui::style::Stylize;
  use ratatui::{text::Line, text::Span};

  let line = Line::from(vec![
      ">_ ".dim(),
      "You are using OpenAI Codex in".bold(),
      format!(" {path_and_branch}").dim(),
  ]);
  ```

- Bold the keyword: split internal budgets when a helper issues multiple commands.
  ```rust
  // Inside collector: divide overall budget across internal git calls
  pub async fn collect_git_info_with_timeout(
      cwd: &std::path::Path,
      per_call_timeout: std::time::Duration,
  ) -> Option<GitInfo> {
      let branch = timeout(per_call_timeout, run_git("rev-parse", &["--abbrev-ref", "HEAD"], cwd))
          .await.ok()??;
      let root = timeout(per_call_timeout, run_git("rev-parse", &["--show-toplevel"], cwd))
          .await.ok()??;
      Some(GitInfo { branch: Some(branch), root: Some(root) })
  }
  ```

**DON'Ts**
- Bold the keyword: don’t bake a single global timeout for every context.
  ```rust
  // Avoid: one-size-fits-all constant used everywhere
  const GIT_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(400);

  // … later …
  let git_info = collect_git_info(&cwd).await; // no callsite control
  ```

- Bold the keyword: don’t block the UI with sync Git queries.
  ```rust
  // Avoid in TUI render path:
  let branch_suffix = codex_core::git_info::collect_git_info_blocking(&config.cwd)
      .ok()
      .and_then(|g| g.branch)
      .map(|b| format!(" ({b})"))
      .unwrap_or_default();
  ```

- Bold the keyword: don’t run long operations sequentially without bounds.
  ```rust
  // Avoid: sequential awaits with no timeouts (slows handshake)
  let git_info = collect_git_info(&cwd).await;
  let (history_log_id, history_entry_count) = message_history::history_metadata(&config).await;
  ```

- Bold the keyword: don’t rely solely on internal per-command timeouts.
  ```rust
  // Avoid: helper hides timing; callsite can’t adjust
  async fn collect_git_info(cwd: &Path) -> Option<GitInfo> {
      // internally picks arbitrary timeouts; callers can’t tune per context
      run_git_command_with_timeout(...).await.ok()?;
      run_git_command_with_timeout(...).await.ok()?;
      // …
  }
  ```

- Bold the keyword: don’t leave placeholder or stray comments in diffs.
  ```rust
  // Avoid:
  //
  // (remove empty or placeholder comment lines before merging)
  ```