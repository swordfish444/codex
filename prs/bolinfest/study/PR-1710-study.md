**DOs**
- **Prefer typed commands over magic ANSI**: Define small `crossterm::Command` wrappers so intent is obvious.
```rust
use std::fmt::{self, Write as _};
use crossterm::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetScrollRegion(pub std::ops::Range<u16>);
impl Command for SetScrollRegion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        // CSI {top};{bottom} r — set scroll region
        write!(f, "\x1b[{};{}r", self.0.start, self.0.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetScrollRegion;
impl Command for ResetScrollRegion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        // CSI r — reset scroll region
        write!(f, "\x1b[r")
    }
}
```

- **Comment raw ANSI if you must use it**: If wrappers aren’t viable, explain the sequence inline.
```rust
// CSI 1;{top} r — restrict scroll area to rows 1..=top
queue!(std::io::stdout(), Print(format!("\x1b[1;{}r", area.top()))).ok();
// CSI r — reset scroll region to full screen
queue!(std::io::stdout(), Print("\x1b[r")).ok();
```

- **Compute wrapped height using Unicode display width**: Count double‑width characters and span concatenation.
```rust
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

fn line_height(line: &Line, width: u16) -> u16 {
    if width == 0 { return 1; }
    let w: usize = line.spans.iter().map(|s| s.content.width()).sum();
    ((w as u16).div_ceil(width)).max(1)
}
```

- **Scroll viewport only when needed and keep it in sync**: Make room when not at the bottom, then update the viewport area.
```rust
use ratatui::layout::Size;

let screen = terminal.backend().size().unwrap_or(Size::new(0, 0));
let mut area = terminal.get_frame().area();

let needed = wrapped_line_count(&lines, area.width);
if area.bottom() < screen.height {
    let n = needed.min(screen.height - area.bottom());
    terminal.backend_mut()
        .scroll_region_down(area.top()..screen.height, n)
        .ok();
    area.y += n;
    terminal.set_viewport_area(area);
}
```

- **Insert lines by limiting the scroll region, printing, then resetting**: Place the cursor at the region end and print CRLF before spans.
```rust
use crossterm::{queue, style::Print};
use ratatui::layout::Position;

queue!(std::io::stdout(), SetScrollRegion(1..area.top())).ok();
terminal.set_cursor_position(Position::new(0, area.top() - 1)).ok();

for line in &lines {
    queue!(std::io::stdout(), Print("\r\n")).ok();
    write_spans(&mut std::io::stdout(), line.iter()).ok();
}

queue!(std::io::stdout(), ResetScrollRegion).ok();
```

- **Inline variables in format/write macros**: Follow house style for clarity.
```rust
write!(f, "\x1b[{};{}r", start, end)?;
```

- **Handle edge cases explicitly**: Guard zero widths and ensure at least one visual line.
```rust
let height = line_height(&line, area.width).max(1);
```


**DON’Ts**
- **Don’t emit unexplained magic ANSI strings**: They’re hard to review and maintain.
```rust
// BAD: unclear intent
queue!(std::io::stdout(), Print("\x1b[1;24r")).ok();
```

- **Don’t approximate wrapping with lines.len()**: This ignores Unicode widths and span composition.
```rust
// BAD: ignores display width and wrapping
let wrapped = lines.len() as u16;
```

- **Don’t move the cursor above the screen**: Large inserts can underflow and place the cursor off‑screen.
```rust
// BAD: can underflow when many lines insert
terminal.set_cursor_position(Position::new(0, area.top() - lines.len() as u16)).ok();
```

- **Don’t forget to reset the scroll region**: Leaving a restricted region breaks later rendering.
```rust
// BAD: missing reset
// queue!(std::io::stdout(), ResetScrollRegion).ok();
```

- **Don’t scroll the whole screen line‑by‑line**: Prefer setting a scroll region and printing once for correctness and speed.
```rust
// BAD: inefficient and fails for multi‑line wraps
terminal.backend_mut().scroll_region_up(0..area.top(), 1).ok();
```

- **Don’t keep unused imports**: Trim them to reduce noise and lints.
```rust
// BAD: unused in this module
use ratatui::prelude::Backend;
```