**DOs**

- Use a dedicated paste event: route terminal paste directly through the app/event pipeline for a single, efficient update.
```
/* app.rs */
if let Event::Paste(pasted) = event {
    app_event_tx.send(AppEvent::Paste(pasted));
}

/* app_event.rs */
pub(crate) enum AppEvent {
    KeyEvent(KeyEvent),
    Paste(String),
    Scroll(i32),
}
```

- Choose a high collapse threshold: start with 1_000 (or at least 500) to avoid replacing everyday pastes and frustrating edits.
```
/* chat_composer.rs */
const LARGE_PASTE_CHAR_THRESHOLD: usize = 1_000;
```

- Keep the UI responsive but preserve real content: insert a short placeholder in the textarea, and store the full text for submission.
```
/* chat_composer.rs */
pub fn handle_paste(&mut self, pasted: String) -> bool {
    let char_count = pasted.chars().count();
    if char_count > LARGE_PASTE_CHAR_THRESHOLD {
        let placeholder = format!("[Pasted Content {char_count} chars]");
        self.textarea.insert_str(&placeholder);
        self.pending_pastes.push((placeholder, pasted));
    } else {
        self.textarea.insert_str(&pasted);
    }
    self.sync_command_popup();
    self.sync_file_search_popup();
    true
}
```

- Expand placeholders on submit: ensure the model receives the original pasted text, not the placeholder.
```
/* chat_composer.rs */
let mut text = self.textarea.lines().join("\n");
for (placeholder, actual) in &self.pending_pastes {
    if text.contains(placeholder) {
        text = text.replace(placeholder, actual);
    }
}
self.pending_pastes.clear();
InputResult::Submitted(text)
```

- Make deletion intuitive: if the cursor is at the end of a placeholder, a single Backspace removes the entire placeholder and its tracking entry.
```
/* chat_composer.rs */
fn try_remove_placeholder_at_cursor(&mut self) -> bool {
    let (row, col) = self.textarea.cursor();
    let line = self.textarea.lines().get(row).map(|s| s.as_str()).unwrap_or("");
    if let Some(ph) = self.pending_pastes.iter().find_map(|(ph, _)| {
        (col >= ph.len() && &line[col - ph.len()..col] == ph).then(|| ph.clone())
    }) {
        for _ in 0..ph.len() {
            self.textarea.input(Input { key: Key::Backspace, ctrl: false, alt: false, shift: false });
        }
        self.pending_pastes.retain(|(p, _)| p != &ph);
        return true;
    }
    false
}
```

- Keep tracking in sync after edits: drop any pending mapping whose placeholder no longer exists in the textarea.
```
/* chat_composer.rs */
let text_after = self.textarea.lines().join("\n");
self.pending_pastes.retain(|(ph, _)| text_after.contains(ph));
```

- Request a redraw when paste changes the UI: wire paste handling through the BottomPane.
```
/* bottom_pane/mod.rs */
pub fn handle_paste(&mut self, pasted: String) {
    if self.active_view.is_none() && self.composer.handle_paste(pasted) {
        self.request_redraw();
    }
}
```

- Use `format!` with inlined variables for clarity and consistency.
```
let placeholder = format!("[Pasted Content {char_count} chars]");
```

- Keep snapshot tests clean and deterministic: start from a fresh composer per case; don’t leak keystrokes between cases; assert the exact frame.
```
#[test]
fn ui_snapshots() {
    let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
    for (name, setup) in [("empty", None), ("large", Some("z".repeat(1_005)))] {
        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender);
        if let Some(p) = setup { composer.handle_paste(p); }
        terminal.draw(|f| f.render_widget_ref(&composer, f.area())).unwrap();
        insta::assert_snapshot!(name, terminal.backend());
    }
}
```

**DON’Ts**

- Don’t set a tiny threshold (e.g., 100): it collapses normal pastes and makes editing painful.
```
/* Too aggressive */
const LARGE_PASTE_CHAR_THRESHOLD: usize = 100;
```

- Don’t synthesize per-character key events for paste: it’s slow, lossy for newlines, and bypasses intentional paste semantics.
```
/* Avoid this pattern */
for ch in pasted.chars() {
    let ev = match ch {
        '\n' | '\r' => KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
        _ => KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()),
    };
    app_event_tx.send(AppEvent::KeyEvent(ev));
}
```

- Don’t ship placeholders to the model: always expand them before submission.
```
/* Buggy: submits placeholder text */
let text = self.textarea.lines().join("\n");
InputResult::Submitted(text) // <-- missing expansion over pending_pastes
```

- Don’t ignore stray characters in snapshots: a leading “t” (or any unexpected glyph) indicates state leakage or setup error.
```
/* Suspicious snapshot: investigate test setup */
"│t[Pasted Content 105 chars] │"
```

- Don’t keep stale pending mappings after edits: if a placeholder is partially or fully removed, drop its mapping.
```
/* Buggy: retains mappings even after placeholder is gone */
self.pending_pastes = self.pending_pastes; // no-op; should filter based on textarea contents
```

- Don’t add a config knob for the threshold prematurely: start with a sensible constant (e.g., 1_000) and adjust based on real-world feedback.