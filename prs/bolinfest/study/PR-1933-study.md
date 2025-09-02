**DOs**
- **Use a single‑line, user‑centric prompt:** Start with a subtle icon and concise copy.
```rust
use ratatui::{style::Color, widgets::ListItem};
use ratatui::style::Stylize;

let cmd = strip_bash_lc_and_escape(&command);
let line = Line::from(vec![
    "? ".fg(Color::Blue),
    "Codex wants to run ".bold(),
    format!("{cmd}").dim(),
]);
```

- **Sanitize and dim the command:** Always render the proposed command safely and de‑emphasized.
```rust
let cmd = strip_bash_lc_and_escape(&command);
lines.push(Line::from(vec![
    "Codex wants to run ".bold(),
    format!("{cmd}").dim(),
]));
```

- **Show explicit decision outcomes:** Use clear verbs and consistent green/red affordances.
```rust
match decision {
    ReviewDecision::Approved => lines.push(Line::from(vec![
        "✓ ".fg(Color::Green), "You ".into(), "approved".bold(),
        " Codex to run ".into(), format!("{cmd}").dim(), " this time".bold(),
    ])),
    ReviewDecision::ApprovedForSession => lines.push(Line::from(vec![
        "✓ ".fg(Color::Green), "You ".into(), "approved".bold(),
        " Codex to run ".into(), format!("{cmd}").dim(),
        " every time this session".bold(),
    ])),
    ReviewDecision::Denied => lines.push(Line::from(vec![
        "✗ ".fg(Color::Red), "You ".into(), "did not approve".bold(),
        " Codex to run ".into(), format!("{cmd}").dim(),
    ])),
    ReviewDecision::Abort => lines.push(Line::from(vec![
        "✗ ".fg(Color::Red), "You ".into(), "canceled".bold(),
        " the request to run ".into(), format!("{cmd}").dim(),
    ])),
}
```

- **Keep the reason optional and separate:** Add a blank line, then the reason when present.
```rust
if let Some(reason) = reason {
    lines.push(Line::from(""));
    lines.push(Line::from(format!("{reason}")));
}
```

- **Prefer Stylize helpers for simple styles:** Use `.bold()`, `.dim()`, `.fg()` instead of manual `Style`.
```rust
// Good
let line = Line::from(vec!["M".green(), " ".dim(), "tui/src/app.rs".dim()]);

// Only drop down to Style for complex cases
```

- **Inline variables with `format!` braces:** Favor interpolation over concatenation.
```rust
// Good
let prompt = format!("Codex wants to run {cmd}");

// Avoid
let prompt = "Codex wants to run ".to_string() + &cmd;
```

**DON’Ts**
- **Don’t duplicate approval messages in history:** Render the prompt in the approval widget; skip background events.
```rust
// Bad: noisy duplication
self.add_to_history(HistoryCell::new_background_event(
    format!("command requires approval:\n$ {cmd}"),
));

// Good: no history log here; the widget displays the request
```

- **Don’t show `cwd` in the prompt:** It adds noise and potential PII; keep the summary clean.
```rust
// Bad
let cwd_str = relativize_to_home(cwd)
    .map_or(cwd.display().to_string(), |rel| format!("~/{}", rel.display()));
lines.push(Line::from(vec![cwd_str.dim(), "$".into(), format!(" {cmd}").into()]));

// Good
lines.push(Line::from(vec!["? ".fg(Color::Blue), "Codex wants to run ".bold(), format!("{cmd}").dim()]));
```

- **Don’t suppress dead‑code warnings—remove the code:** Delete unused helpers rather than `#[allow(dead_code)]`.
```rust
// Before (avoid keeping)
#[allow(dead_code)]
pub(crate) fn new_background_event(message: String) -> Self { /* ... */ }

// After: remove the unused function; rely on Git history if needed.
```

- **Don’t hand‑roll styles for simple spans:** Avoid `Span::styled` or mutating `Span.style` when `.dim()`/`.bold()` suffice.
```rust
// Bad
let span = {
    let mut s: Span = cmd.clone().into();
    s.style = s.style.add_modifier(Modifier::DIM);
    s
};

// Good
let span = format!("{cmd}").dim();
```

- **Don’t use vague copy:** Prefer clear verbs (“approved”, “did not approve”, “canceled”) over generic labels.
```rust
// Good
"✓ ".fg(Color::Green); "approved".bold();

// Avoid
"decision: Approved".into();
```