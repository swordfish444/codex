**DOs**
- Use `poll()` before `read()`: Check for input with a short timeout so the event reader doesn’t hold Crossterm’s global event lock, which can block `cursor::position()` and cause resize crashes.
```rust
use std::time::{Duration, Instant};
use crossterm::event::{self, Event};

const EVENT_POLL: Duration = Duration::from_millis(100);

loop {
    if event::poll(EVENT_POLL)? {
        match event::read()? {
            Event::Key(k) => app_event_tx.send(AppEvent::KeyEvent(k)),
            Event::Resize(_, _) => app_event_tx.send(AppEvent::RequestRedraw),
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => scroll_event_helper.scroll_up(),
                MouseEventKind::ScrollDown => scroll_event_helper.scroll_down(),
                _ => {}
            },
            Event::Paste(p) => app_event_tx.send(AppEvent::Paste(p.replace("\r", "\n"))),
            _ => {}
        }
    } else {
        // No input this tick; do lightweight work or continue.
    }
}
```

- Keep timeout comments precise: Explain why `poll()` is used and distinguish the poll interval from Crossterm’s separate lock timeout used by `cursor::position()`.
```rust
// Poll every 100ms to avoid holding Crossterm’s event lock.
// cursor::position() may wait up to ~2s to acquire that lock;
// if the reader holds it continuously, position reads can fail.
```

- Cite upstream patterns: Add a brief comment linking to Ratatui’s own example so future maintainers recognize this is a known, recommended pattern.
```rust
// Pattern inspired by Ratatui inline example:
// https://github.com/ratatui/ratatui/blob/9836f0760d4a053d9d1eba78171be89cb22dc850/examples/apps/inline/src/main.rs#L98-L118
```

- Model Ratatui’s tick/input loop: Optionally separate “ticks” from input so the UI can update periodically even without events (closer to Ratatui examples).
```rust
use std::time::{Duration, Instant};
use crossterm::event::{self, Event};

const TICK_RATE: Duration = Duration::from_millis(200);

let mut last_tick = Instant::now();
loop {
    let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(k) => app_event_tx.send(AppEvent::KeyEvent(k)),
            Event::Resize(_, _) => app_event_tx.send(AppEvent::RequestRedraw),
            _ => {}
        }
    }
    if last_tick.elapsed() >= TICK_RATE {
        app_event_tx.send(AppEvent::Tick);
        last_tick = Instant::now();
    }
}
```

- Use named durations to avoid confusion: Make differing timeouts explicit and keep comments in sync.
```rust
const EVENT_POLL: Duration = Duration::from_millis(100);
const CURSOR_LOCK_TIMEOUT_DOC: &str = "cursor::position() may wait ~2s for event lock";
```

**DON’Ts**
- Don’t call `read()` in a tight loop: This can monopolize the event lock and make `cursor::position()` fail during resizes.
```rust
// BAD: holds the event lock continuously
while let Ok(event) = crossterm::event::read() {
    handle(event); // Other code needing the event lock may starve
}
```

- Don’t write ambiguous timeout comments: Avoid mixing numbers (e.g., “2 sec” vs “100ms”) without clarifying what each applies to.
```rust
// BAD: “Use 100ms so we don’t hit the 2s timeout”
// Which timeout? For what call? Be explicit.
```

- Don’t diverge from proven upstream patterns without reason: If you implement a different loop structure, explain why it’s needed for your app.
```rust
// If not following Ratatui’s poll+read+tick pattern,
// document the rationale and implications for event lock usage.
```

- Don’t redraw lazily on resize: Make resize explicit and cheap—request a redraw promptly.
```rust
match event {
    crossterm::event::Event::Resize(_, _) => app_event_tx.send(AppEvent::RequestRedraw),
    _ => {}
}
```