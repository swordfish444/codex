**DOs**
- **Alpha‑sort feature lists:** Keep Cargo feature arrays sorted for quick scanning.
```toml
# Cargo.toml
[dependencies]
ratatui = { version = "0.29.0", features = [
  "scrolling-regions",
  "unstable-rendered-line-info",
  "unstable-widget-ref",
] }
```
- **Surface public API first:** Place `pub` items before helpers for faster discovery.
```rust
// insert_history.rs
pub(crate) fn insert_history_lines(term: &mut tui::Tui, lines: Vec<Line<'static>>) {
    // ...
}

fn write_spans(w: &mut impl Write, spans: impl Iterator<Item = &'_ Span<'_>>) -> io::Result<()> {
    // ...
}
```
- **Use clear, meaningful defaults (no magic numbers):** Prefer named constants over hex literals.
```rust
let screen_height = term.backend().size().map(|s| s.height).unwrap_or(u16::MAX);
```
- **Finish comments and explain “why”:** Make rationale explicit, not implied.
```rust
// We scroll one line at a time because terminals cannot place the cursor
// above the top of the visible screen. This guarantees every printed line
// lands within the valid region.
```
- **Handle modifier diffs symmetrically:** Apply both removals and additions when changing styles.
```rust
let removed = from - to;
if removed.contains(Modifier::BOLD) { queue!(w, SetAttribute(CAttribute::NormalIntensity))?; }
if removed.contains(Modifier::ITALIC) { queue!(w, SetAttribute(CAttribute::NoItalic))?; }
// ... other removals

let added = to - from;
if added.contains(Modifier::BOLD) { queue!(w, SetAttribute(CAttribute::Bold))?; }
if added.contains(Modifier::ITALIC) { queue!(w, SetAttribute(CAttribute::Italic))?; }
// ... other additions
```
- **Reset styles after writing:** Always restore terminal colors and attributes.
```rust
queue!(
    w,
    SetForegroundColor(CColor::Reset),
    SetBackgroundColor(CColor::Reset),
    SetAttribute(crossterm::style::Attribute::Reset),
)?;
```
- **If you own the type, enable exhaustive handling via iteration:** Consider deriving iteration on your enum to cover all variants in tests.
```toml
# Cargo.toml
strum = "0.26"
strum_macros = "0.26"
```
```rust
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(EnumIter, Debug, Clone, Copy)]
enum MyStyle { Bold, Italic, Underline /* ... */ }

for s in MyStyle::iter() {
    // assert that s is handled
}
```

**DON’Ts**
- **Don’t use Paragraph to wrap scrollback history:** Let the terminal wrap; use scrolling regions and direct writes instead of a `Buffer`.
```rust
// ❌ Avoid
terminal.insert_before(total_lines, |buf| {
    Paragraph::new(line).render(area, buf);
});

// ✅ Prefer
terminal.backend_mut().scroll_region_up(0..top, 1)?;
terminal.set_cursor_position(Position::new(0, top - 1))?;
write_spans(&mut std::io::stdout(), line.iter())?;
```
- **Don’t bury public functions below helpers:** Avoid making readers hunt for the entry point.
```rust
// ❌ Helpers first, public last
fn helper_a() {}
fn helper_b() {}
pub(crate) fn insert_history_lines(...) {}
```
- **Don’t leave comments unfinished or ambiguous:** Remove placeholders like “unfini” and complete the thought.
```rust
// ❌ Incomplete
// We scroll up one line at a time because

// ✅ Complete
// We scroll one line at a time to avoid writing above the screen top.
```
- **Don’t rely on unexplained magic numbers:** Replace literals like `0xffffu16` with clear constants.
```rust
// ❌
let screen_height = size.map(|s| s.height).unwrap_or(0xffffu16);

// ✅
let screen_height = size.map(|s| s.height).unwrap_or(u16::MAX);
```
- **Don’t partially update style state:** Changing only added attributes (or only removed) causes drift.
```rust
// ❌ Only adds, never clears
if added.contains(Modifier::BOLD) { queue!(w, SetAttribute(CAttribute::Bold))?; }

// ✅ Apply removals and additions
let removed = from - to; /* clear removed */
let added = to - from;   /* set added  */
```
- **Don’t add enum-iteration deps you can’t use:** `strum` helps only when you own the enum; for third‑party bitflags, use explicit handling and tests instead.
```rust
// ❌ Adding strum to iterate a third-party bitflags type (won’t work)

// ✅ Keep explicit mapping + tests around handled flags
assert!(handles_modifier(Modifier::BOLD));
assert!(handles_modifier(Modifier::ITALIC));
```