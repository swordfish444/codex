**DOs**
- **Prefer safe signal wrappers:** Use `nix::sys::signal::kill` instead of manual `unsafe`.
```rust
#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

#[cfg(unix)]
fn suspend(&mut self, terminal: &mut tui::Tui) -> Result<()> {
    tui::restore()?;
    let _ = kill(Pid::from_raw(0), Signal::SIGTSTP);
    *terminal = tui::init(&self.config)?;
    terminal.clear()?;
    self.app_event_tx.send(AppEvent::RequestRedraw);
    Ok(())
}
```

- **Restore, suspend, then re-init and redraw:** Leave the terminal clean before/after job control.
```rust
#[cfg(unix)]
fn suspend(&mut self, terminal: &mut tui::Tui) -> Result<()> {
    tui::restore()?;
    let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(0), nix::sys::signal::Signal::SIGTSTP);
    *terminal = tui::init(&self.config)?;
    terminal.clear()?;
    self.app_event_tx.send(AppEvent::RequestRedraw);
    Ok(())
}
```

- **Gate suspension to Unix and scope deps with cfg:** Keep platform-specific code and dependencies isolated.
```toml
# codex-rs/tui/Cargo.toml
[target.'cfg(unix)'.dependencies]
nix = { version = "0.29", default-features = false, features = ["signal"] }
```
```rust
match key {
    KeyEvent { code: KeyCode::Char('z'), modifiers: KeyModifiers::CONTROL, kind: KeyEventKind::Press, .. } => {
        #[cfg(unix)]
        { self.suspend(terminal)?; }
        // Non-Unix: no-op
    }
    _ => {}
}
```

- **Trigger on Ctrl-Z press only:** Avoid repeat actions on key repeat/release.
```rust
if let KeyEvent {
    code: KeyCode::Char('z'),
    modifiers: KeyModifiers::CONTROL,
    kind: KeyEventKind::Press,
    ..
} = key {
    #[cfg(unix)]
    { self.suspend(terminal)?; }
}
```

- **Send SIGTSTP to the process group:** Use PID 0 for standard job-control semantics.
```rust
#[cfg(unix)]
let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(0), nix::sys::signal::Signal::SIGTSTP);
```

**DON’Ts**
- **Don’t call `unsafe libc::kill` directly:** Prefer a safe wrapper to avoid manual `unsafe`.
```rust
// Avoid:
#[cfg(unix)]
unsafe { libc::kill(0, libc::SIGTSTP) };
```

- **Don’t repurpose Ctrl-Z to “interrupt tasks”:** Reserve Ctrl-Z for suspend; use Ctrl-C/Esc for interrupts.
```rust
// Avoid:
if let AppState::Chat { widget } = &mut self.app_state {
    widget.on_ctrl_z(); // previously interrupted running task
}
```

- **Don’t suspend without restoring first:** Restoring before sending SIGTSTP prevents a wedged terminal.
```rust
// Avoid (order is wrong):
let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(0), nix::sys::signal::Signal::SIGTSTP);
tui::restore()?; // too late
```

- **Don’t emulate suspend on non-Unix:** Keep it a no-op instead of custom behavior.
```rust
// Avoid:
#[cfg(not(unix))]
self.suspend(terminal)?; // incorrect: non-Unix should do nothing
```

- **Don’t leave the UI stale after resume:** Re-init, clear, and request a redraw.
```rust
// Avoid: missing re-init/clear/redraw after resume
#[cfg(unix)]
fn suspend(&mut self, _terminal: &mut tui::Tui) -> Result<()> {
    tui::restore()?;
    let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(0), nix::sys::signal::Signal::SIGTSTP);
    Ok(()) // UI will be stale
}
```