**DOs**

- **Explain Scoring Constant:** Add a clear comment for the prefix bonus and how the score is computed.
```rust
// Score is "extra span" outside the needle between first/last hits.
// Lower is better. Strongly reward prefix matches to surface commands quickly.
let mut score = window.max(0);
/*
Prefix bonus rationale:
-100 is large enough to dominate small window differences, so a prefix match
beats a slightly tighter non-prefix match but not a wildly better one.
*/
if first_lower_pos == 0 {
    score -= 100; // prefix bonus
}
```

- **Assert Scores In Tests:** Treat tests as executable documentation by checking both indices and scores.
```rust
#[test]
fn prefer_contiguous_prefix() {
    let (idx, score) = fuzzy_match("abc", "abc").expect("match");
    assert_eq!(idx, vec![0, 1, 2]);
    assert_eq!(score, -100); // contiguous prefix → window 0 + prefix bonus
}

#[test]
fn spread_match_is_worse() {
    let (_idx, score) = fuzzy_match("a-b-c", "abc").expect("match");
    assert_eq!(score, -98); // window 2 - 100 bonus
    // Prefix contiguous (-100) still beats spread (-98).
}
```

- **Centralize Popup Limits:** Keep popup row limits in one place and use them consistently.
```rust
// popup_consts.rs
pub(crate) const MAX_POPUP_ROWS: usize = 8;

// usage
let vis = MAX_POPUP_ROWS.min(matches_len);
state.ensure_visible(matches_len, vis);
```

- **Cache Filtered Results:** Recompute only when the filter or source data changes; reuse on up/down.
```rust
pub(crate) struct CommandPopup {
    command_filter: String,
    all_commands: Vec<(&'static str, SlashCommand)>,
    cached: Vec<(&'static SlashCommand, Option<Vec<usize>>, i32)>,
    state: ScrollState,
}

impl CommandPopup {
    fn rebuild_cache(&mut self) {
        self.cached = self.compute_filtered(); // called on filter change only
        let len = self.cached.len();
        self.state.clamp_selection(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }

    pub(crate) fn move_down(&mut self) {
        let len = self.cached.len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }
}
```

- **Clamp And Keep Selection Visible:** Always clamp selection to bounds and update scroll.
```rust
let len = rows.len();
state.clamp_selection(len);
state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
```

- **Adjust Highlight Indices When Prefixing:** If you add a "/" or other prefix to the displayed name, shift indices.
```rust
// internal indices are for "help"
let indices = fuzzy_indices("help", "hp").unwrap();
// UI shows "/help" → shift by 1 so bold aligns with UI text
let shifted: Vec<usize> = indices.into_iter().map(|i| i + 1).collect();
```

- **Use ratatui Stylize Helpers:** Prefer concise styling over manual Style building.
```rust
use ratatui::style::Stylize;

let spans = vec![
    "/".into(),
    "help".bold(),       // matched chars can be bolded one-by-one
    "  ".into(),
    "Show help".dim(),   // description is dim gray
];

// Selected row styling
let cell = Cell::from(Line::from(spans)).style("".yellow().bold().style());
```

- **Deterministic Sorting:** Sort by score, then by command for stability.
```rust
out.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.command().cmp(b.0.command())));
```

- **Preserve “Searching…” State:** Distinguish waiting from no-results to avoid confusing users.
```rust
if waiting && rows_all.is_empty() {
    // show an explicit searching state
    let row = Row::new(vec![Cell::from(Line::from("(searching …)".dim()))]);
    Table::new(vec![row], [Constraint::Percentage(100)]).render(area, buf);
} else {
    render_rows(area, buf, &rows_all, &state, MAX_POPUP_ROWS);
}
```

- **Keep Comments Accurate:** If a helper is used at runtime, don’t label it “tests only”.
```rust
/// Returns filtered commands for both UI rendering and tests.
fn filtered_commands(&self) -> Vec<&SlashCommand> { /* ... */ }
```

- **Make Wrap-Around Explicit:** Gate wrap-around behavior for clarity and easier UX tweaks.
```rust
pub(crate) struct ScrollState {
    pub selected_idx: Option<usize>,
    pub scroll_top: usize,
    pub wrap: bool,
}

pub fn move_down(&mut self, len: usize) {
    if len == 0 { self.selected_idx = None; return; }
    self.selected_idx = Some(match (self.selected_idx, self.wrap) {
        (Some(i), _) if i + 1 < len => i + 1,
        (_, true) => 0,
        (Some(i), false) => i,
        (None, _) => 0,
    });
}
```

- **Be Explicit About Unicode Behavior:** Dedup indices after lowercase expansion (e.g., İ → i̇).
```rust
result_orig_indices.sort_unstable();
result_orig_indices.dedup(); // safe for multi-char case mappings
```


**DON’Ts**

- **Don’t Leave Magic Numbers Unexplained:** Avoid bare “-100” without comment or rationale.
```rust
// BAD
if first_lower_pos == 0 { score -= 100; }
```

- **Don’t Skip Score Assertions:** Verifying only indices omits critical ranking behavior.
```rust
// BAD: indices only
let (idx, _score) = fuzzy_match("hello", "hl").unwrap();
assert_eq!(idx, vec![0, 2]);
```

- **Don’t Recompute On Every Keypress:** Avoid re-filtering on Up/Down when input hasn’t changed.
```rust
// BAD
pub fn move_up(&mut self) {
    let matches = self.compute_filtered(); // unnecessary work per keypress
}
```

- **Don’t Let Selection Drift Out Of Range:** Always clamp after data changes; otherwise panics or stale UI can occur.
```rust
// BAD: no clamp, potential OOB
self.state.selected_idx = Some(self.state.selected_idx.unwrap() + 1);
```

- **Don’t Show “no matches” While Searching:** Users read it as terminal, not in-progress.
```rust
// BAD
if matches.is_empty() { show("no matches"); }
```

- **Don’t Keep Unused Fields:** Remove or wire `is_current`; dead fields confuse future maintainers.
```rust
// BAD: never set, always false
pub is_current: bool,
```

- **Don’t Bypass Stylize:** Avoid verbose Style plumbing when helpers are available.
```rust
// BAD
Span::styled("text", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
```

- **Don’t Use Unstable Ordering:** Ties without deterministic tiebreakers jitter the UI.
```rust
// BAD: score ties produce arbitrary order
out.sort_by(|a, b| a.2.cmp(&b.2));
```

- **Don’t Forget Prefix Offset For Highlights:** Your bolding will be misaligned.
```rust
// BAD: indices for "help" applied to "/help" without +1 shift
```

- **Don’t Assume Wrap-Around Is Always Desired:** Either make it configurable or document it explicitly.
```rust
// BAD: hardcoded wrap-around with no way to disable
self.state.move_down_wrap(len);
```