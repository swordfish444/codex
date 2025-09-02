**Burst Paste Edge Cases: Review Guide (PR #2683)**

**DOs**
- Bold the keyword, then colon + concise description below each, with a short code example.

- Idempotent setters: perform side effects only when state changes.
```rust
fn set_disable_paste_burst(&mut self, disabled: bool) {
    let was_disabled = self.disable_paste_burst;
    self.disable_paste_burst = disabled;
    if disabled && !was_disabled {
        self.paste_burst.clear_window_after_non_char();
    }
}
```

- Constructor wiring: pass config flags into `new()` and centralize side effects via a setter.
```rust
impl ChatComposer {
    pub fn new(..., disable_paste_burst: bool) -> Self {
        let mut this = Self { /* fields */, paste_burst: PasteBurst::default(), disable_paste_burst: false };
        this.set_disable_paste_burst(disable_paste_burst);
        this
    }
}
```

- Doc comments: use `///`, describe behavior and return values succinctly.
```rust
/// Flush buffered content if the inter-key timeout elapsed.
/// Returns Some(buffered_or_saved_text) when a flush occurs, otherwise None.
pub fn flush_if_due(&mut self, now: Instant) -> Option<String> { /* ... */ }
```

- Clear, accurate names: use descriptive enum variants (e.g., `RetainFirstChar`).
```rust
pub(crate) enum CharDecision {
    BeginBuffer { retro_chars: u16 },
    BufferAppend,
    RetainFirstChar,
    BeginBufferFromPending,
}
```

- Unicode-safe heuristics: count characters, not bytes; document rationale.
```rust
/// Begin buffering if recent text likely came from a paste (whitespace or long).
let looks_pastey = grabbed.chars().any(|c| c.is_whitespace()) || grabbed.chars().count() >= 16;
```

- Tight, total returns: ensure boolean helpers return on all paths; prefer single expression.
```rust
pub(crate) fn handle_paste_burst_tick(&mut self, fr: FrameRequester) -> bool {
    if self.bottom_pane.flush_paste_burst_if_due() {
        self.request_redraw();
        true
    } else if self.bottom_pane.is_in_paste_burst() {
        fr.schedule_frame_in(ChatComposer::recommended_paste_flush_delay());
        true
    } else {
        false
    }
}
```

- Friendly imports: import types once; avoid fully qualified paths where possible.
```rust
use std::time::Duration;

pub(crate) fn request_redraw_in(&self, dur: Duration) {
    self.frame_requester.schedule_frame_in(dur);
}
```

- Smart redraw scheduling: request immediate redraw on flush; schedule a delayed tick while capturing.
```rust
if needs_redraw { self.request_redraw(); }
if self.composer.is_in_paste_burst() {
    self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
}
```

- Simple tests: keep helpers minimal; human-like typing with a small delay; use simple literals.
```rust
fn type_chars_humanlike(c: &mut ChatComposer, chars: &[char]) {
    for &ch in chars {
        let _ = c.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        std::thread::sleep(ChatComposer::recommended_paste_flush_delay());
        let _ = c.flush_paste_burst_if_due();
    }
}

let count = 32;
```

- Exact fixtures: keep field names and timestamp formats precise.
```json
// Right
{"ts":"2025-08-09T15:58:15.433Z","dir":"to_tui","kind":"app_event","variant":"RequestRedraw"}
// Wrong
{"ts":"2025-08-09T15:58:15.4F33Z","fdir":"to_tui","kind":"app_event","variant":"RequestRedraw"}
```

**DON’Ts**
- Repeat side effects: don’t re-clear state when the flag was already set.
```rust
// Don’t: always clear even if already disabled.
self.disable_paste_burst = disabled;
if disabled { self.paste_burst.clear_window_after_non_char(); }
```

- Use `//` where `///` is required: don’t skimp on doc comments or omit return-value notes.
```rust
// Don’t
// Timer: flush buffered burst if timeout elapsed.
```

- Hide config outside constructors: don’t require callers to set flags post-init without a clear reason.
```rust
// Don’t
let mut c = ChatComposer::new(...);
c.set_disable_paste_burst(true); // Wire it through new() instead.
```

- Fully qualify types repeatedly: don’t write long paths when an import suffices.
```rust
// Don’t
pub fn request_redraw_in(&self, dur: std::time::Duration) { /* ... */ }
```

- Count bytes for heuristics: don’t use `.len()` on `String` slices for thresholds.
```rust
// Don’t
let looks_pastey = grabbed.contains(' ') || grabbed.len() >= 16;
```

- Leave partial returns: don’t forget a return value in one branch.
```rust
// Don’t
if flushed { self.request_redraw(); /* missing return */ }
```

- Overuse `#[cfg(test)]` inside `mod tests`: don’t gate internals unnecessarily within a test module.
```rust
// Don’t
#[cfg(test)]
fn helper_in_mod_tests_only() { /* ... */ }
```

- Introduce fixture typos: don’t change keys (`dir` → `fdir`) or corrupt timestamps.
```json
// Don’t
{"ts":"...15.4F33Z","fdir":"to_tui", ...}
```

- Redundant renders: don’t render and also schedule a follow-up in the same frame when a flush just happened.
```rust
// Don’t
self.draw_now();
fr.schedule_frame_in(Duration::from_millis(1));
```

- Aggressive retro-grab: don’t yank preceding text unless the heuristic clearly indicates paste.
```rust
// Don’t
begin_with_retro_grabbed(before.to_string(), now); // without checking whitespace/length
```