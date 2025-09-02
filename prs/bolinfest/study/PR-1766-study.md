**DOs**

- **Prefer per-instance state**: Pass terminal capability into UI components; avoid global mutables.
```
struct TuiState { kkp_enabled: bool }

impl TuiState {
    fn new(kkp_enabled: bool) -> Self { Self { kkp_enabled } }
}

let state = TuiState::new(detect_kkp()?);
let composer = ChatComposer::with_state(true, sender.clone(), state);
```

- **Initialize once, then freeze**: If caching capability globally, set it once with `OnceLock` and never mutate.
```
static KKP_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn init_capabilities() {
    let kkp = detect_kkp().unwrap_or(false);
    let _ = KKP_ENABLED.set(kkp);
}

fn is_kkp_enabled() -> bool {
    *KKP_ENABLED.get().unwrap_or(&false)
}
```

- **Use dependency injection in tests**: Make tests deterministic by supplying the capability explicitly.
```
let state_shift = TuiState::new(true);
let state_ctrlj = TuiState::new(false);

let composer_shift = ChatComposer::with_state(true, sender.clone(), state_shift);
let composer_ctrlj = ChatComposer::with_state(true, sender.clone(), state_ctrlj);
```

- **Detect features with portable APIs**: Prefer `crossterm` polling and short timeouts; guard OS-specific code.
```
#[cfg(unix)]
fn detect_kkp() -> std::io::Result<bool> {
    use std::{io::{self, Write}, time::Duration};
    use crossterm::{event, ExecutableCommand};
    io::stdout().execute(crossterm::style::Print("\x1b[?u\x1b[c"))?;
    io::stdout().flush()?;
    if event::poll(Duration::from_millis(150))? {
        // Read raw bytes or translate from crossterm events here.
        return Ok(true); // Parse sequences as needed.
    }
    Ok(false)
}

#[cfg(not(unix))]
fn detect_kkp() -> std::io::Result<bool> { Ok(false) }
```

- **Reflect capability in hints**: Compute the hint once and style with `Stylize`.
```
use ratatui::style::Stylize;

let newline_hint = if state.kkp_enabled { "Shift+⏎" } else { "Ctrl+J" };
let line = vec!["⏎".into(), " send   ".into(), newline_hint.cyan(), " newline".into()];
```

- **Keep snapshots stable**: Capture both capability states explicitly and name snapshots clearly.
```
assert_snapshot!("composer_shift_enter", render(&composer_shift));
assert_snapshot!("composer_ctrl_j", render(&composer_ctrlj));
```

- **Inline variables in format!**: Use braces for values in messages and errors.
```
return Err(anyhow::anyhow!("Failed to draw composer: {e}"));
```


**DON’Ts**

- **Don’t mutate global flags in tests**: Avoid `static AtomicBool` toggled per test; it invites race conditions and flaky snapshots.
```
/* Bad */
static KKP_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn set_kkp_for_tests(v: bool) { KKP_ENABLED.store(v, std::sync::atomic::Ordering::Relaxed); }
```

- **Don’t block initialization**: No long or unbounded waits for terminal responses; keep detection fast or skip.
```
/* Bad */
std::thread::sleep(std::time::Duration::from_secs(2)); // blocks TUI startup
```

- **Don’t assume KKP is present**: Always provide a fallback accelerator and hint when KKP is missing.
```
/* Bad */
let newline_hint = "Shift+⏎"; // hardcoded, no fallback
```

- **Don’t rely on unguarded OS-specific APIs**: Avoid unconditional `libc` calls; use `#[cfg]` guards or portable crates.
```
/* Bad */
let rc = unsafe { libc::poll(pfd, 1, 200) }; // no cfg guard, non-portable
```

- **Don’t couple tests to ambient environment**: Tests should not depend on terminal/TTY; inject capability instead of probing.
```
/* Bad */
let kkp = detect_kkp().unwrap_or(false); // runs in CI, may flake
```

- **Don’t share mutable test state across snapshots**: If global state is unavoidable, serialize tests; better yet, remove the shared state.
```
/* Acceptable fallback when refactor isn’t possible */
#[serial_test::serial]
#[test]
fn snapshots_with_global_state() { /* ... */ }
```