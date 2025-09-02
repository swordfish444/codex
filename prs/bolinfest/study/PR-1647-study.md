**DOs**
- **Name Events Precisely:** Use ShutdownComplete for the completion notification; keep Op::Shutdown for requests.
```rust
// protocol.rs
pub enum Op {
    // ...
    Shutdown,
}

pub enum EventMsg {
    // ...
    ShutdownComplete,
}
```

- **Gracefully Drain Rollout Writer:** Add a Shutdown command with a oneshot ack; don’t invent “Sync/Drain” that flushes unnecessarily.
```rust
// rollout.rs
enum RolloutCmd {
    AddItems(Vec<ResponseItem>),
    UpdateState(SessionStateSnapshot),
    Shutdown { ack: oneshot::Sender<()> },
}

async fn rollout_writer(mut file: File, mut rx: Receiver<RolloutCmd>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            RolloutCmd::AddItems(items) => { /* write items; flush as needed */ }
            RolloutCmd::UpdateState(state) => { /* write state; flush as needed */ }
            RolloutCmd::Shutdown { ack } => { let _ = ack.send(()); } // no extra flush
        }
    }
}
```

- **Propagate Errors Cleanly:** Prefer match and return Errs instead of swallowing them.
```rust
// rollout.rs
pub async fn shutdown(&self) -> std::io::Result<()> {
    let (tx_done, rx_done) = oneshot::channel();
    match self.tx.send(RolloutCmd::Shutdown { ack: tx_done }).await {
        Ok(()) => rx_done
            .await
            .map_err(|e| IoError::other(format!("failed waiting for rollout shutdown: {e}"))),
        Err(e) => Err(IoError::other(format!("failed to send rollout shutdown command: {e}"))),
    }
}
```

- **Take Ownership Only When Needed:** Use Option::take() when you must ensure the recorder is dropped after shutdown to unblock the writer.
```rust
// codex.rs (on Op::Shutdown)
if let Some(sess_arc) = sess {
    let recorder_opt = sess_arc.rollout.lock().unwrap().take(); // take so we can drop after await
    if let Some(rec) = recorder_opt {
        if let Err(e) = rec.shutdown().await {
            warn!("failed to shutdown rollout recorder: {e}");
            // send error EventMsg if needed
        }
        // rec drops here; tx closes; writer can finish naturally
    }
}
```

- **Centralize “Last Message” Writing:** Keep a single helper and call it from both processors. Pass the path via constructors, not per-call.
```rust
// event_processor.rs
pub(crate) fn handle_last_message(msg: Option<&str>, path: Option<&Path>) {
    match (path, msg) {
        (Some(p), Some(m)) => { let _ = std::fs::write(p, m); }
        (Some(p), None) => {
            let _ = std::fs::write(p, "");
            eprintln!("Warning: no last agent message; wrote empty content to {}", p.display());
        }
        (None, _) => eprintln!("Warning: no file to write last message to."),
    }
}

// exec processors (constructors)
impl EventProcessorWithHumanOutput {
    pub(crate) fn create_with_ansi(with_ansi: bool, config: &Config, last_message_path: Option<PathBuf>) -> Self { /* store path */ }
}
impl EventProcessorWithJsonOutput {
    pub fn new(last_message_path: Option<PathBuf>) -> Self { /* store path */ }
}
```

- **Map Events To Status Clearly:** Return CodexStatus from process_event; early-return for non-Running, default to Running.
```rust
// event_processor.rs
pub(crate) enum CodexStatus { Running, InitiateShutdown, Shutdown }

// event_processor_with_human_output.rs
fn process_event(&mut self, event: Event) -> CodexStatus {
    match event.msg {
        EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) => {
            handle_last_message(last_agent_message.as_deref(), self.last_message_path.as_deref());
            return CodexStatus::InitiateShutdown;
        }
        EventMsg::ShutdownComplete => return CodexStatus::Shutdown,
        // ... print other events
        _ => {}
    }
    CodexStatus::Running
}
```

- **Drive Shutdown From The Main Loop:** Submit Op::Shutdown on InitiateShutdown; break on Shutdown.
```rust
// exec/lib.rs
while let Some(event) = rx.recv().await {
    match event_processor.process_event(event) {
        CodexStatus::Running => continue,
        CodexStatus::InitiateShutdown => { codex.submit(Op::Shutdown).await?; }
        CodexStatus::Shutdown => break,
    }
}
```

- **Exit TUI On ShutdownComplete:** Let Ctrl-C request shutdown; exit only after ShutdownComplete.
```rust
// chatwidget.rs (handling events)
EventMsg::ShutdownComplete => {
    self.app_event_tx.send(AppEvent::ExitRequest);
}

// chatwidget.rs (Ctrl-C path)
} else if self.bottom_pane.ctrl_c_quit_hint_visible() {
    self.submit_op(Op::Shutdown);
    true
} else {
    self.bottom_pane.show_ctrl_c_quit_hint();
    false
}
```

- **Keep Comments Useful:** Use comments to explain intent (e.g., why take()), not to restate code.

**DON’Ts**
- **Don’t Swallow Errors:** Avoid returning Ok(()) on send/await failures; propagate them.
```rust
// Avoid:
if let Err(e) = self.tx.send(cmd).await { return Ok(()); }
```

- **Don’t Over-Engineer Draining:** Skip “Sync/Drain” variants that flush again; the writer already flushes on write paths. The Shutdown ack is sufficient.

- **Don’t Duplicate Helper Logic:** Don’t inline “last message” file writes in multiple places; call handle_last_message instead.

- **Don’t Pass Per-Call Paths:** Don’t thread last_message_file through process_event; store it in the processor at construction time.

- **Don’t Extend Traits Needlessly:** Don’t add last_message_path getters or file-writing methods to the EventProcessor trait just to share code; use top-level helpers.

- **Don’t Change UX Lightly:** Don’t alter Ctrl-C semantics (e.g., removing immediate ExitRequest) without manual testing and ensuring the TUI waits for ShutdownComplete.

- **Don’t Add Redundant Comments:** Avoid “This is a no-op.” when the code already makes that obvious.

- **Don’t Force Writer Exit:** Don’t terminate the writer inside Shutdown; let it finish naturally when tx is dropped after shutdown completes.