**Asynchronous Diff UI: DOs and DON’Ts**

**DOs**
- **Use tokio::spawn for work:** Offload long-running tasks to keep the UI responsive.
```rust
let tx = self.app_event_tx.clone();
tokio::spawn(async move {
    let text = match get_git_diff().await {
        Ok((true, diff)) => diff,
        Ok((false, _)) => "`/diff` — _not inside a git repository_".to_string(),
        Err(e) => format!("Failed to compute diff: {e}"),
    };
    let _ = tx.send(AppEvent::DiffResult(text));
});
```

- **Make helpers async:** Switch to `tokio::process::Command` and `.await` process I/O.
```rust
use std::{io, process::Stdio};
use tokio::process::Command;

async fn run_git_capture_diff(args: &[&str]) -> io::Result<String> {
    let output = Command::new("git").args(args)
        .stdout(Stdio::piped()).stderr(Stdio::null())
        .output().await?;
    if output.status.success() || output.status.code() == Some(1) {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, format!("git {:?} failed", args)))
    }
}
```

- **Report results via AppEvent:** Define and handle a dedicated event for the async result.
```rust
// app_event.rs
pub(crate) enum AppEvent {
    // ...
    DiffResult(String),
}

// app.rs
AppEvent::DiffResult(text) => {
    if let AppState::Chat { widget } = &mut self.app_state {
        widget.add_diff_output(text);
    }
}
```

- **Show a progress indicator:** Set a running state and update status text while the task runs.
```rust
// chatwidget.rs
pub(crate) fn add_diff_in_progress(&mut self) {
    self.bottom_pane.set_task_running(true);
    self.bottom_pane.update_status_text("computing diff".to_string());
    self.request_redraw();
}

pub(crate) fn add_diff_output(&mut self, diff_output: String) {
    self.bottom_pane.set_task_running(false);
    self.add_to_history(&history_cell::new_diff_output(diff_output));
    self.mark_needs_redraw();
}
```

- **Gate status updates through the view:** Only update when the status view is active.
```rust
// bottom_pane/mod.rs
pub(crate) fn update_status_text(&mut self, text: String) {
    if !self.is_task_running || !self.status_view_active {
        return;
    }
    if let Some(mut view) = self.active_view.take() {
        view.update_status_text(text);
        self.active_view = Some(view);
        self.request_redraw();
    }
}
```

- **Parallelize independent subprocesses:** Use `tokio::join!` and `JoinSet` for concurrency.
```rust
// run tracked diff + list untracked in parallel
let (tracked_diff_res, untracked_res) = tokio::join!(
    run_git_capture_diff(&["diff", "--color"]),
    run_git_capture_stdout(&["ls-files", "--others", "--exclude-standard"]),
);
let tracked_diff = tracked_diff_res?;
let untracked_list = untracked_res?;

// diff untracked files concurrently
let null_path = "/dev/null".to_string();
let mut join_set = tokio::task::JoinSet::new();
for f in untracked_list.lines().map(str::trim).filter(|s| !s.is_empty()) {
    let f = f.to_string();
    let null_path = null_path.clone();
    join_set.spawn(async move {
        run_git_capture_diff(&["diff","--color","--no-index","--",&null_path,&f]).await
    });
}
let mut untracked_diff = String::new();
while let Some(res) = join_set.join_next().await {
    match res {
        Ok(Ok(diff)) => untracked_diff.push_str(&diff),
        Ok(Err(e)) if e.kind() == io::ErrorKind::NotFound => {},
        Ok(Err(e)) => return Err(e),
        Err(_) => {}
    }
}
```

- **Check repo state first:** Short-circuit gracefully when not inside a Git repo.
```rust
async fn inside_git_repo() -> io::Result<bool> {
    let status = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null()).stderr(Stdio::null())
        .status().await;
    match status {
        Ok(s) if s.success() => Ok(true),
        Ok(_) => Ok(false),
        Err(e) => Err(e),
    }
}
```

**DON’Ts**
- **Don’t block the UI thread:** Avoid synchronous process I/O in the event loop.
```rust
// Bad: blocks the reactor and UI
let out = std::process::Command::new("git").arg("diff").output().unwrap();

// Good: async process
let out = tokio::process::Command::new("git").arg("diff").output().await?;
```

- **Don’t spawn std threads in Tokio contexts:** Prefer `tokio::spawn` to integrate with the runtime.
```rust
// Bad
std::thread::spawn(move || { /* ... */ });

// Good
tokio::spawn(async move { /* ... */ });
```

- **Don’t mutate UI from background tasks:** Send an event; handle all UI updates on the main thread.
```rust
// Bad
tokio::spawn(async move {
    widget.add_diff_output("...".to_string()); // UI from background
});

// Good
tokio::spawn(async move {
    let _ = tx.send(AppEvent::DiffResult("...".to_string()));
});
```

- **Don’t forget to end the progress state:** Always clear `task_running` when the task completes.
```rust
// Ensure this runs on completion paths
self.bottom_pane.set_task_running(false);
```

- **Don’t ignore Git’s special exit codes:** Treat diff exit code 1 as success; surface real failures.
```rust
// Correct handling
if output.status.success() || output.status.code() == Some(1) {
    // differences present or clean
} else {
    return Err(io::Error::new(io::ErrorKind::Other, format!("git {:?} failed", args)));
}
```