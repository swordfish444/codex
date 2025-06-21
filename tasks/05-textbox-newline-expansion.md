# Task 05: Multi-line Composer Support (Ctrl+J & Auto-Expand)

## Goal
Enable users to insert newlines in the chat prompt without submitting the message when they press `Ctrl+J`. The composer box should auto-expand vertically up to a configurable maximum height.

## Acceptance Criteria
- Pressing `Ctrl+J` when focused in the prompt adds a newline instead of submitting.
- The composer widget grows in height as lines wrap or newlines are inserted, up to a max height (e.g. 10 lines).
- Below the max height, the input box uses a scrollbar or stops expanding.
- Behavior is consistent across windows and respects TUI resize events.

## Notes
- This requires changes in `tui/src/bottom_pane/chat_composer.rs` logic and layout in `tui/src/bottom_pane/bottom_pane_view.rs`.