**DOs**

- Bold: Debounce redraws via a request event: Introduce `RequestRedraw` and schedule `Redraw` after ~100 ms.
```rust
// app_event.rs
pub(crate) enum AppEvent {
    // ...
    RequestRedraw,
    Redraw,
    // ...
}

// app.rs
const REDRAW_DEBOUNCE: Duration = Duration::from_millis(100);
```

- Bold: Scope locks tightly: Lock only to check/set the pending flag, then drop it before spawning the worker.
```rust
fn schedule_redraw(&self) {
    {
        let mut flag = self.pending_redraw.lock().unwrap();
        if *flag {
            return;
        }
        *flag = true;
    } // lock dropped here

    let tx = self.app_event_tx.clone();
    let pending_redraw = self.pending_redraw.clone();
    std::thread::spawn(move || {
        std::thread::sleep(REDRAW_DEBOUNCE);
        tx.send(AppEvent::Redraw);
        let mut f = pending_redraw.lock().unwrap();
        *f = false;
    });
}
```

- Bold: Prefer `.clone()` ergonomics: Clone `Arc`s and senders with `.clone()` before moving into threads.
```rust
let tx = self.app_event_tx.clone();
let pending_redraw = self.pending_redraw.clone();
std::thread::spawn(move || {
    // use tx, pending_redraw here
});
```

- Bold: Convert producers to `RequestRedraw`: Replace direct `Redraw` sends in all producers.
```rust
// On startup
self.app_event_tx.clone().send(AppEvent::RequestRedraw);

// On terminal resize
crossterm::event::Event::Resize(_, _) => {
    app_event_tx.send(AppEvent::RequestRedraw);
}

// Bottom pane / widgets
self.app_event_tx.send(AppEvent::RequestRedraw);
```

- Bold: Handle the request distinctly: Turn `RequestRedraw` into a debounced `Redraw` in the main loop.
```rust
while let Ok(event) = self.app_event_rx.recv() {
    match event {
        AppEvent::RequestRedraw => self.schedule_redraw(),
        AppEvent::Redraw => self.draw_next_frame(terminal)?,
        _ => {}
    }
}
```

- Bold: Keep helpers near first use: Place `schedule_redraw` just above the match block that calls it for readability.
```rust
impl App<'_> {
    // ... fields, new(), etc.

    // place here (near run/loop)
    fn schedule_redraw(&self) { /* as above */ }

    pub(crate) fn run(&mut self, terminal: &mut tui::Tui, mouse_capture: &mut MouseCapture) -> Result<()> {
        // uses schedule_redraw in match
    }
}
```

**DON’Ts**

- Bold: Don’t hold locks across work: Avoid sleeping, sending, or spawning while a `MutexGuard` is alive.
```rust
// BAD: lock held far too long
fn schedule_redraw_bad(&self) {
    let mut flag = self.pending_redraw.lock().unwrap();
    if *flag { return; }
    *flag = true;

    std::thread::sleep(REDRAW_DEBOUNCE); // holding the lock
    self.app_event_tx.send(AppEvent::Redraw); // still holding the lock
}
```

- Bold: Don’t emit `Redraw` directly from producers: Always send `RequestRedraw` so the app can debounce.
```rust
// BAD
app_event_tx.send(AppEvent::Redraw);

// GOOD
app_event_tx.send(AppEvent::RequestRedraw);
```

- Bold: Don’t reschedule when one is pending: Guard with a boolean to coalesce rapid requests.
```rust
// Inside schedule_redraw
let mut flag = self.pending_redraw.lock().unwrap();
if *flag { return; } // don't stack timers
*flag = true;
```

- Bold: Don’t bury relevant helpers: Avoid scattering debounce logic far from where events are handled.
```rust
// BAD: helper lives in a distant module/file, hard to follow
// GOOD: helper is defined in the same impl, just above its call sites
```

- Bold: Don’t prefer long-lived patterns when simple clones work: Use `.clone()` in place of `Arc::clone(&x)` when clarity is equal.
```rust
// Preferred here
let pending_redraw = self.pending_redraw.clone();

// Avoid when it adds noise
let pending_redraw = std::sync::Arc::clone(&self.pending_redraw); // noisier
```