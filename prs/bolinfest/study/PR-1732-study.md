**DOs**
- **Comment Rationale**: Add a brief comment when using low‑level cursor ops that deliberately bypass `Terminal` state.
```rust
// Using MoveTo instead of terminal.set_cursor_position to avoid mutating
// last_known_cursor_pos, which the viewport logic relies on.
queue!(stdout, MoveTo(0, cursor_top))?;
```

- **Use `MoveTo` For Ephemeral Moves**: Temporarily reposition the cursor with `crossterm::queue!` so `last_known_cursor_pos` stays accurate.
```rust
use crossterm::{queue, cursor::MoveTo};
use std::io::{self, Write};

let mut stdout = io::stdout();
queue!(stdout, MoveTo(0, cursor_top))?;
```

- **Preserve Cursor‑Position Neutrality**: Capture and restore the cursor position around history insertion.
```rust
let saved = terminal.get_cursor_position().ok();
// ... emit lines above viewport ...
if let Some(pos) = saved {
    queue!(stdout, MoveTo(pos.x, pos.y))?;
}
```

- **Bracket Scroll Region Changes**: Always pair `SetScrollRegion` with `ResetScrollRegion`.
```rust
use crossterm::terminal::{SetScrollRegion, ResetScrollRegion};
queue!(stdout, SetScrollRegion(1..area.top()))?;
// ... writes that rely on the scroll region ...
queue!(stdout, ResetScrollRegion)?;
```

- **Keep Comments Close To The Action**: Place the explanatory comment immediately above the `MoveTo`/restore call so intent is unmistakable.
```rust
// insert_history_lines should be cursor-position-neutral.
queue!(stdout, MoveTo(pos.x, pos.y))?;
```

**DON’Ts**
- **Don’t Mutate Terminal State For Temporary Moves**: Avoid `terminal.set_cursor_position(...)` when you don’t intend to change `last_known_cursor_pos`.
```rust
// ❌ Avoid: this updates last_known_cursor_pos and can desync viewport math.
terminal.set_cursor_position((0, cursor_top))?;
```

- **Don’t Forget To Restore The Cursor**: Leaving the cursor at the injection point can confuse subsequent rendering.
```rust
// ❌ Missing restore leaves cursor in the wrong place
// Do: capture -> write -> restore (see DOs above).
```

- **Don’t Leave The Scroll Region Dirty**: Failing to reset it affects all future draws.
```rust
// ❌ Missing ResetScrollRegion
queue!(stdout, SetScrollRegion(1..area.top()))?;
// Do: always follow with ResetScrollRegion.
```

- **Don’t Skip Explaining Non‑Obvious Choices**: If you bypass higher‑level APIs (e.g., `Terminal::set_cursor_position`), document why.
```rust
// ❌ No comment explaining why low-level cursor ops are used
// Do: add a one-liner clarifying the state-preservation intent.
```