**DOs**
- **Compute Byte Offsets From Char Cursor**: Convert a char-based cursor column to a safe byte offset before slicing.
```rust
let cursor_byte = line.chars().take(col).map(|c| c.len_utf8()).sum::<usize>();
let before = &line[..cursor_byte];
let after  = &line[cursor_byte..];
```

- **Find Boundaries With `char_indices` + `is_whitespace`**: Use Unicode-aware whitespace and advance by the found char’s byte length.
```rust
let start = before
    .char_indices()
    .rfind(|(_, c)| c.is_whitespace())
    .map(|(i, c)| i + c.len_utf8())
    .unwrap_or(0);

let end_rel = after
    .char_indices()
    .find(|(_, c)| c.is_whitespace())
    .map(|(i, _)| i)
    .unwrap_or(after.len());

let end = cursor_byte + end_rel;
```

- **Honor All Unicode Whitespace**: Treat tabs and full-width spaces as token boundaries.
```rust
let full_width_space = '\u{3000}'; // IDEOGRAPHIC SPACE
assert!(full_width_space.is_whitespace());
```

- **Early-Return On Invalid Spans**: Bail out if the computed range is empty or not an @-token.
```rust
if start >= end { return None; }
let token = &line[start..end];
if !token.starts_with('@') || token == "@" { return None; }
```

- **Replace Using Byte Slices And Inline Formatting**: Build the new line with the computed byte indices and captured identifiers.
```rust
let new_line = format!("{}{} {}", &line[..start], replacement, &line[end..]);
```

- **Use Captured Identifiers In Messages**: Prefer inline names in `assert!`/`format!` rather than positional args.
```rust
assert_eq!(result, expected, "Failed: {description} - input: '{input}', cursor: {cursor_pos}");
```

- **Test ASCII + Unicode Thoroughly**: Cover mixed scripts, emoji, tabs, and full-width spaces.
```rust
let cases = vec![
    ("@İstanbul", 3, Some("İstanbul".to_string())),
    ("test\u{3000}@file", 6, Some("file".to_string())),
    ("aaa@aaa", 4, None),
];
for (input, cursor_pos, expected) in cases {
    let mut ta = tui_textarea::TextArea::default();
    ta.insert_str(input);
    ta.move_cursor(tui_textarea::CursorMove::Jump(0, cursor_pos));
    let result = ChatComposer::current_at_token(&ta);
    assert_eq!(result, expected, "input='{input}', cursor={cursor_pos}");
}
```

**DON’Ts**
- **Don’t Mix Char Columns With Byte Indices**: Never clamp a char column with `line.len()` (bytes).
```rust
// wrong
let col = col.min(line.len());

// right
let cursor_byte = line.chars().take(col).map(|c| c.len_utf8()).sum::<usize>();
```

- **Don’t Add A Fixed `+1` For Multibyte Chars**: Advance by the matched char’s byte length.
```rust
// wrong
let start = before.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);

// right
let start = before.char_indices().rfind(|(_, c)| c.is_whitespace())
    .map(|(i, c)| i + c.len_utf8()).unwrap_or(0);
```

- **Don’t Assume ASCII Space Only**: Searching for `' '` misses tabs and CJK spaces.
```rust
// wrong
let end_rel = after.find(' ').unwrap_or(after.len());

// right
let end_rel = after.char_indices()
    .find(|(_, c)| c.is_whitespace())
    .map(|(i, _)| i)
    .unwrap_or(after.len());
```

- **Don’t Slice With A Char Index**: Convert to a byte offset first.
```rust
// wrong
let before = &line[..col]; // col is chars, not bytes

// right
let cursor_byte = line.chars().take(col).map(|c| c.len_utf8()).sum::<usize>();
let before = &line[..cursor_byte];
```

- **Don’t Treat Connected Tokens As Mentions**: Require a boundary before `@`.
```rust
use tui_textarea::{TextArea, CursorMove};
let mut ta = TextArea::default();
ta.insert_str("aaa@aaa");
ta.move_cursor(CursorMove::Jump(0, 4));
assert_eq!(ChatComposer::current_at_token(&ta), None);
```

- **Don’t Use Positional Formatting When Capture Works**: Prefer `{name}` over `{}` with args.
```rust
// wrong
assert_eq!(result, expected, "Failed: {} - '{}'", description, input);

// right
assert_eq!(result, expected, "Failed: {description} - '{input}'");
```