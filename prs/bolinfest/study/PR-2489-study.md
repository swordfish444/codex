**DOs**
- Bold keyword: Integrate tokio EventStream for input; multiplex with app events using `tokio::select!`.
```rust
use crossterm::event::EventStream;
use tokio::select;
use tokio_stream::StreamExt;

let mut crossterm_events = EventStream::new();

while let Some(app_ev) = {
    select! {
        maybe = app_event_rx.recv() => maybe, // App events (our channel)
        Some(Ok(ev)) = crossterm_events.next() => match ev {
            crossterm::event::Event::Key(k) => Some(AppEvent::KeyEvent(k)),
            crossterm::event::Event::Resize(..) => Some(AppEvent::Redraw),
            crossterm::event::Event::Paste(p) => Some(AppEvent::Paste(p.replace("\r", "\n"))),
            _ => None,
        },
    }
} { handle_event(terminal, app_ev)?; }
```

- Bold keyword: Use `tokio::sync::mpsc::unbounded_channel` for app events (and tests).
```rust
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender, UnboundedReceiver};

let (tx, rx): (UnboundedSender<AppEvent>, UnboundedReceiver<AppEvent>) = unbounded_channel();

#[derive(Clone, Debug)]
pub(crate) struct AppEventSender { pub app_event_tx: UnboundedSender<AppEvent> }

impl AppEventSender {
    pub fn send(&self, ev: AppEvent) { let _ = self.app_event_tx.send(ev); }
}
```

- Bold keyword: Factor event logic into a reusable handler that returns “keep running”.
```rust
fn handle_event(&mut self, terminal: &mut tui::Tui, ev: AppEvent) -> Result<bool> {
    match ev {
        AppEvent::RequestRedraw => self.schedule_frame_in(REDRAW_DEBOUNCE),
        AppEvent::Redraw => std::io::stdout().sync_update(|_| self.draw_next_frame(terminal))??,
        AppEvent::ExitRequest | AppEvent::DispatchCommand(SlashCommand::Quit) => return Ok(false),
        _ => { /* other cases... */ }
    }
    Ok(true)
}
```

- Bold keyword: Normalize pasted text to LF; many terminals paste CR.
```rust
match event {
    crossterm::event::Event::Paste(p) => Some(AppEvent::Paste(p.replace("\r", "\n"))),
    _ => None,
}
```

- Bold keyword: Debounce redraws to coalesce frames.
```rust
const REDRAW_DEBOUNCE: Duration = Duration::from_millis(1);

self.app_event_tx.send(AppEvent::RequestRedraw);
// ...
AppEvent::RequestRedraw => self.schedule_frame_in(REDRAW_DEBOUNCE),
```

- Bold keyword: Restore cursor using tracked coordinates, not a live query (avoids event-lock issues).
```rust
use crossterm::cursor::MoveTo;
use crossterm::queue;

queue!(
    writer,
    MoveTo(terminal.last_known_cursor_pos.x, terminal.last_known_cursor_pos.y)
).ok();
```

- Bold keyword: Keep key semantics consistent and predictable.
```rust
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

match key_event {
    KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, kind: KeyEventKind::Press, .. } => {
        self.app_event_tx.send(AppEvent::ExitRequest);
    }
    KeyEvent { code: KeyCode::Char('d'), modifiers: KeyModifiers::CONTROL, kind: KeyEventKind::Press, .. } => {
        if widget.composer_is_empty() { self.app_event_tx.send(AppEvent::ExitRequest); }
        else { self.dispatch_key_event(key_event); }
    }
    KeyEvent { kind: KeyEventKind::Press | KeyEventKind::Repeat, .. } => self.dispatch_key_event(key_event),
    _ => {} // Ignore KeyEventKind::Release
}
```

- Bold keyword: Make entry points async and await the run loop.
```rust
pub async fn run_main(cli: Cli, config: Config, show_trust: bool) -> std::io::Result<()> {
    run_ratatui_app(cli, config, show_trust).await
        .map_err(|e| std::io::Error::other(e.to_string()))
}

async fn run_ratatui_app(cli: Cli, config: Config, show_trust: bool) -> eyre::Result<()> {
    let mut app = App::new(config.clone(), cli.prompt, cli.images, show_trust);
    app.run(&mut terminal).await?;
    Ok(())
}
```

- Bold keyword: Explain non-obvious changes that are related but not obvious (e.g., cursor restore).
```text
PR note: Switched to EventStream; querying cursor position can contend with crossterm’s event lock.
Use last_known_cursor_pos to restore cursor without locking; fixes resize-time failures.
```

- Bold keyword: Map terminal resize → redraw, not a full rerender loop.
```rust
crossterm::event::Event::Resize(..) => Some(AppEvent::Redraw)
```

**DON’Ts**
- Bold keyword: Don’t block on `crossterm::event::read()` or hold the event lock.
```rust
// ❌ Blocking pattern (avoids async multiplexing and can deadlock):
if crossterm::event::poll(Duration::from_millis(100))? {
    let ev = crossterm::event::read()?; // holds event lock
}
```

- Bold keyword: Don’t use `std::sync::mpsc` in async code and tests.
```rust
// ❌
let (tx, rx) = std::sync::mpsc::channel::<AppEvent>();

// ✅
let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();
```

- Bold keyword: Don’t call `cursor::position()` or `terminal.get_cursor_position()` during rendering.
```rust
// ❌ May contend with crossterm’s event lock:
let cursor_pos = terminal.get_cursor_position()?;

// ✅ Use tracked position:
MoveTo(terminal.last_known_cursor_pos.x, terminal.last_known_cursor_pos.y)
```

- Bold keyword: Don’t exit on Ctrl+D if the composer has content.
```rust
// ❌ Always exits:
if ctrl_d { return Ok(false); }

// ✅ Only exit when input is empty:
if ctrl_d && widget.composer_is_empty() { return Ok(false); }
```

- Bold keyword: Don’t handle `KeyEventKind::Release`; it causes duplicate actions.
```rust
match key_event.kind {
    KeyEventKind::Press | KeyEventKind::Repeat => self.dispatch_key_event(key_event),
    _ => {} // ignore Release
}
```

- Bold keyword: Don’t forget Cargo feature flags for crossterm’s event stream.
```toml
# ✅ Cargo.toml
crossterm = { version = "0.28.1", features = ["bracketed-paste", "event-stream"] }
tokio-stream = "0.1.17"
```

- Bold keyword: Don’t leave tests using `try_iter()` from std mpsc; use `try_recv()` in a loop.
```rust
// ✅ With tokio channel:
let mut events = Vec::new();
while let Ok(ev) = rx.try_recv() { events.push(ev); }
```

- Bold keyword: Don’t mix unrelated changes without context; add a brief rationale.
```text
Commit message: “Use tracked cursor pos after EventStream refactor”
Rationale: Avoids event-lock contention introduced by async event handling.
```