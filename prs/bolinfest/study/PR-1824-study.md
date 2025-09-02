**DOs**
- Factor layout logic into a helper: centralize the split between the transient “active history cell” and the bottom pane, and reuse it in both `cursor_pos()` and `render_ref()`.
```rust
impl ChatWidget<'_> {
    fn layout_areas(&self, area: Rect) -> [Rect; 2] {
        Layout::vertical([
            Constraint::Max(
                self.active_history_cell
                    .as_ref()
                    .map_or(0, |c| c.desired_height(area.width)),
            ),
            Constraint::Min(self.bottom_pane.desired_height(area.width)),
        ])
        .areas(area)
    }

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        let [_, bottom] = self.layout_areas(area);
        self.bottom_pane.cursor_pos(bottom)
    }
}

impl WidgetRef for &ChatWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let [active, bottom] = self.layout_areas(area);
        (&self.bottom_pane).render(bottom, buf);
        if let Some(cell) = &self.active_history_cell {
            cell.render_ref(active, buf);
        }
    }
}
```

- Pass structured events to constructors: prefer handing `HistoryCell` the full event (e.g., `PatchApplyEndEvent`) instead of plucking out fields at the call site.
```rust
// ChatWidget handler
EventMsg::PatchApplyEnd(event) => {
    self.add_to_history(HistoryCell::new_patch_apply_end(event));
}

// HistoryCell API
pub(crate) fn new_patch_apply_end(event: PatchApplyEndEvent) -> Self {
    let success = event.success;
    let stdout = event.stdout;
    let stderr = event.stderr;
    // ... build lines based on success/stdout/stderr ...
    HistoryCell::PatchApplyResult { view: TextBlock::new(lines) }
}
```

- Be stingy with patch-apply output: success paths generally have non-empty `stdout`; avoid repeating content already shown earlier and limit any additional output to a small snippet.
```rust
pub(crate) fn new_patch_apply_end(event: PatchApplyEndEvent) -> Self {
    let status = if event.success {
        "patch applied".green()
    } else {
        "patch failed".red().bold()
    };

    let mut lines = vec![
        Line::from(vec!["patch".magenta().bold(), " ".into(), status]),
        "".into(),
    ];

    // Optional, terse snippet only if helpful.
    let src = if event.success {
        &event.stdout // success should have useful stdout
    } else if event.stderr.trim().is_empty() {
        &event.stdout
    } else {
        &event.stderr
    };

    if !src.trim().is_empty() {
        for l in src.lines().take(5) {
            lines.push(ansi_escape_line(l).dim());
        }
        let remaining = src.lines().count().saturating_sub(5);
        if remaining > 0 {
            lines.push(Line::from(format!("... {remaining} additional lines")).dim());
        }
        lines.push("".into());
    }

    HistoryCell::PatchApplyResult { view: TextBlock::new(lines) }
}
```

**DON’Ts**
- Don’t duplicate layout computations in multiple methods: avoid copy-pasting the same `Layout::vertical([...]).areas(area)` block in `cursor_pos()` and `render_ref()`.
```rust
// Avoid this duplication in multiple places:
let [active_cell_area, bottom_pane_area] = Layout::vertical([
    Constraint::Max(self.active_history_cell.as_ref().map_or(0, |c| c.desired_height(area.width))),
    Constraint::Min(self.bottom_pane.desired_height(area.width)),
]).areas(area);
// ... the same block repeated elsewhere ...
```

- Don’t peel event fields at the call site just to rewrap them: it scatters knowledge of the event shape and increases churn.
```rust
// Anti-pattern: extracting fields here instead of passing the event
EventMsg::PatchApplyEnd(event) => {
    self.add_to_history(HistoryCell::new_patch_apply_end(
        event.stdout, event.stderr, event.success,
    ));
}
```

- Don’t over-emit patch output or choose the wrong stream: on success, don’t show `stderr`; avoid flooding the UI with long logs right after an apply-begin summary.
```rust
// Anti-patterns:
// 1) Showing stderr on success or assuming stdout might be empty on success
let src = if event.success { &event.stderr } else { &event.stdout }; // wrong

// 2) Dumping unbounded output
for l in src.lines() {
    lines.push(Line::from(l.to_string())); // too verbose; no limit
}
```