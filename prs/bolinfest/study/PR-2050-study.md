**DOs**
- **Centralize wrap width**: Define one `DEFAULT_WRAP_COLS` and reuse across modules.
```rust
// tui/src/common.rs
pub(crate) const DEFAULT_WRAP_COLS: u16 = 80;

// tui/src/chatwidget.rs
use crate::common::DEFAULT_WRAP_COLS;
let mut live_builder = RowBuilder::new(DEFAULT_WRAP_COLS.into());

// tui/src/diff_render.rs
use crate::common::DEFAULT_WRAP_COLS;
let term_cols: usize = crossterm::terminal::size()
    .map(|(w, _)| w as usize)
    .unwrap_or(DEFAULT_WRAP_COLS.into());
```

- **Use enumerate for “first item?” logic**: Avoid extra booleans.
```rust
for (index, (path, change)) in changes.iter().enumerate() {
    let is_first = index == 0;
    if !is_first {
        out.push(Line::from(vec!["    ".into(), "...".dim()]));
    }
    // ...
}
```

- **Compute decimal digit length efficiently**: Prefer `ilog10` when available.
```rust
fn decimal_len(n: usize) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
}
```

- **Prefer format! for small string assembly**: It’s clearer and fast enough.
```rust
let display = match sign_opt {
    Some(sign) => format!("{sign}{chunk}"),
    None => chunk.to_string(),
};
```

- **Test UI with snapshots**: Add `.snap` tests to validate layout quickly.
```rust
#[test]
fn ui_snapshot_add_details() {
    let lines = create_diff_summary("proposed patch", &changes, PatchEventType::ApprovalRequest);
    let mut term = Terminal::new(TestBackend::new(80, 10)).expect("term");
    let cell = HistoryCell::PendingPatch { view: TextBlock::new(lines) };
    term.draw(|f| f.render_widget_ref(&cell, f.area())).expect("draw");
    insta::assert_snapshot!("add_details", term.backend());
}
```

- **Capture style when it matters**: Use a helper to JSON-snapshot styled runs.
```rust
#[derive(serde::Serialize)]
struct StyledSpan { text: String, fg: Option<String>, bg: Option<String>, mods: Vec<&'static str> }

#[derive(serde::Serialize)]
struct Snapshot { width: u16, height: u16, lines: Vec<Vec<StyledSpan>> }

// Convert ratatui::Buffer to Snapshot by compressing same-style runs...
// (sketch) insta::assert_json_snapshot!("paragraph_styled_spans", &buffer_to_snapshot(&buf));
```

- **Align with Ratatui types**: Keep `u16` where the API expects it.
```rust
pub(crate) fn new_completed_mcp_tool_call(
    num_cols: u16, // stays u16 to match ratatui dimensions
    invocation: McpInvocation,
    duration: Duration,
    success: bool,
) -> Self { /* ... */ }
```

- **Be robust parsing unified diffs**: Fall back to manual counts if parsing fails.
```rust
let count_from_unified = |diff: &str| -> (usize, usize) {
    if let Ok(patch) = diffy::Patch::from_str(diff) {
        patch.hunks().iter().flat_map(|h| h.lines()).fold((0, 0), |(a, d), l| match l {
            diffy::Line::Insert(_) => (a + 1, d),
            diffy::Line::Delete(_) => (a, d + 1),
            _ => (a, d),
        })
    } else {
        diff.lines().fold((0, 0), |(a, d), l| {
            if l.starts_with("+++") || l.starts_with("---") || l.starts_with("@@") { (a, d) }
            else if l.starts_with('+') { (a + 1, d) }
            else if l.starts_with('-') { (a, d + 1) }
            else { (a, d) }
        })
    }
};
```

- **Keep styling concise**: Chain where it improves readability.
```rust
line.spans.iter_mut().for_each(|s| s.style = s.style.add_modifier(Modifier::DIM));
```

- **Render signs with the same background as edits**: Improves visual diff clarity.
```rust
let (sign_opt, style_opt) = match kind {
    DiffLineType::Insert => (Some('+'), Some(Style::default().bg(Color::Green))),
    DiffLineType::Delete => (Some('-'), Some(Style::default().bg(Color::Red))),
    DiffLineType::Context => (None, None),
};
let display = sign_opt.map_or_else(|| chunk.to_string(), |s| format!("{s}{chunk}"));
let span = style_opt.map_or_else(|| Span::raw(display.clone()), |st| Span::styled(display.clone(), st));
```

**DON’Ts**
- **Don’t hardcode magic widths**: Avoid ad-hoc values like `96`; don’t duplicate constants.
```rust
// ❌ Bad
const DEFAULT_WRAP_COLS: usize = 96;

// ✅ Good
use crate::common::DEFAULT_WRAP_COLS; // 80, single source of truth
```

- **Don’t track “first file” with a mutable flag**: Use `enumerate()` instead.
```rust
// ❌ Bad
let mut first = true;
for (path, change) in changes {
    if !first { /* separator */ }
    first = false;
}

// ✅ Good
for (i, _) in changes.iter().enumerate() {
    if i > 0 { /* separator */ }
}
```

- **Don’t hand-build tiny strings char-by-char**: Prefer `format!` for clarity.
```rust
// ❌ Bad
let mut s = String::with_capacity(1 + chunk.len());
s.push(sign);
s.push_str(chunk);

// ✅ Good
let s = format!("{sign}{chunk}");
```

- **Don’t change public parameter types away from Ratatui**: Avoid needless `usize` churn.
```rust
// ❌ Bad
pub(crate) fn new_completed_mcp_tool_call(num_cols: usize, /* ... */) { /* ... */ }

// ✅ Good
pub(crate) fn new_completed_mcp_tool_call(num_cols: u16, /* ... */) { /* ... */ }
```

- **Don’t rely on glyph-only snapshots when color/style matters**: You’ll miss regressions.
```rust
// ❌ Only terminal text snapshot; styles lost
insta::assert_snapshot!("widget_text_only", term.backend());

// ✅ Add style-aware JSON snapshot alongside
insta::assert_json_snapshot!("widget_styled", &buffer_to_snapshot(&buf));
```

- **Don’t drop fallback logic on diff parsing**: Unparseable diffs must still yield counts.
```rust
// ❌ Bad: assumes parse always succeeds
let patch = diffy::Patch::from_str(diff).unwrap();

// ✅ Good: parse-or-scan fallback
let (adds, dels) = count_from_unified(diff);
```

- **Don’t duplicate hardcoded spacing rules inline**: Use named constants.
```rust
// ❌ Bad
let gap = 6usize.saturating_sub(ln_str.len());

// ✅ Good
const SPACES_AFTER_LINE_NUMBER: usize = 6;
let gap = SPACES_AFTER_LINE_NUMBER.saturating_sub(ln_str.len());
```