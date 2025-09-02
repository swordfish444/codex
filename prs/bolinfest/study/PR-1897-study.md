**DOs**

- Remove unused fields instead of suppressing them.
```rust
// Good: drop unused field
pub(crate) struct CommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}
```

- Use inline variable capture with format! when possible.
```rust
let cmd = strip_bash_lc_and_escape(&command);
let line = Line::from(format!("$ {cmd}"));
```

- Prefer enumerate() over a mutable “is_first” flag.
```rust
for (idx, raw) in src.lines().take(TOOL_CALL_MAX_LINES).enumerate() {
    let mut line = ansi_escape_line(raw);
    let prefix = if idx == 0 { "  ⎿ " } else { "    " };
    line.spans.insert(0, prefix.into());
    lines.push(line);
}
```

- Sanitize and escape commands shown in the UI.
```rust
let command_escaped = strip_bash_lc_and_escape(&command);
lines.push(Line::from(vec!["⚡Ran command ".magenta(), command_escaped.into()]));
```

- Select stdout on success, stderr on failure.
```rust
let src = if exit_code == 0 { stdout } else { stderr };
```

- Truncate output and show a clear continuation indicator.
```rust
let mut iter = src.lines();
for (idx, raw) in iter.by_ref().take(TOOL_CALL_MAX_LINES).enumerate() {
    let mut line = ansi_escape_line(raw);
    let prefix = if idx == 0 { "  ⎿ " } else { "    " };
    line.spans.insert(0, prefix.into());
    line.spans.iter_mut().for_each(|s| s.style = s.style.add_modifier(Modifier::DIM));
    lines.push(line);
}
let remaining = iter.count();
if remaining > 0 {
    let mut more = Line::from(format!("... +{remaining} lines"));
    more.spans.insert(0, "    ".into());
    more.spans.iter_mut().for_each(|s| s.style = s.style.add_modifier(Modifier::DIM));
    lines.push(more);
}
```

- Use concise ratatui Stylize helpers for basic styling.
```rust
use ratatui::style::Stylize;

let header = Line::from(vec!["▌ ".cyan(), "Running command ".magenta(), command_escaped.into()]);
```

- Keep the output limit constant small; consider Config to make it overridable in a future PR.
```rust
const TOOL_CALL_MAX_LINES: usize = 3; // TODO: Consider making this configurable.
```


**DON’Ts**

- Don’t silence dead code with attributes; remove the code instead.
```rust
// Bad: keeping unused field with a lint escape
pub(crate) struct CommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    #[allow(dead_code)]
    pub(crate) duration: Duration, // not used anywhere
}
```

- Don’t maintain a mutable is_first flag when enumerate() suffices.
```rust
// Bad
let mut is_first = true;
for raw in src.lines().take(TOOL_CALL_MAX_LINES) {
    let prefix = if is_first { "  ⎿ " } else { "    " };
    is_first = false;
    // ...
}
```

- Don’t duplicate the command line or show unused metadata.
```rust
// Bad: redundant title and unused duration/code details
lines.push(Line::from(vec![
    "command".magenta(),
    format!(" (code: {exit_code}, duration: {duration})").dim(),
]));
lines.push(Line::from(format!("$ {command_escaped}")));
```

- Don’t print unbounded output in the UI.
```rust
// Bad: can flood the UI
for raw in src.lines() {
    lines.push(ansi_escape_line(raw));
}
```

- Don’t forget to dim captured output and continuation markers for readability.
```rust
// Bad: lacks visual hierarchy; output blends with UI chrome
let mut line = ansi_escape_line(raw);
lines.push(line); // not dimmed; no prefixes
```