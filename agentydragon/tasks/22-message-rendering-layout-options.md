+++
id = "22"
title = "Message Separation and Sender-Content Layout Options"
status = "Done"
dependencies = "" # No prerequisites
last_updated = "2025-06-25T11:05:55.000000"
+++

## Summary
Add configurable options for inter-message spacing and sender-content line breaks in chat rendering.

## Goal
Provide users with flexibility in how chat messages are visually separated and how sender labels are displayed relative to message content:
- Control whether an empty line is inserted between consecutive messages.
- Control whether sender and content appear on the same line or on separate lines.

## Acceptance Criteria

- Introduce two new config flags under the UI section:
  - `message_spacing: true|false` controls inserting a blank line between messages when true.
  - `sender_break_line: true|false` controls breaking line after the sender label when true.
- Both flags default to `false` to preserve current compact layout.
- When `message_spacing` is enabled, render an empty line between each message bubble or block.
- When `sender_break_line` is enabled, render the sender label on its own line above the message content; otherwise render `Sender: Content` on a single line.
- Ensure both flags can be toggled independently and work together in any combination.
- Add unit tests to verify the four layout permutations produce the correct sequence of lines.

## Implementation
### Plan

- Add `message_spacing` and `sender_break_line` flags to the TUI config schema with default `false`.
- Update the TUI renderer (`history_cell.rs`) to conditionally insert blank lines between messages and break sender/content lines based on these flags.
- Document both flags under the `[tui]` section in `codex-rs/config.md`.
- Ensure existing unit tests in `message_layout.rs` cover all flag combinations and adjust or add tests if needed.

## Notes

- These options improve readability for users who prefer more visual separation or clearer sender labels.
- Keep default settings unchanged to avoid surprising existing users.
