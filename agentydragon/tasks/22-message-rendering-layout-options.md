+++
id = "22"
title = "Message Separation and Sender-Content Layout Options"
status = "Not started"
dependencies = "" # No prerequisites
last_updated = "2025-06-25T01:40:09.600000"
+++

## Summary
Add configurable options for inter-message spacing and sender-content line breaks in chat rendering
**in the codex-rs package** - **NOT** the codex-cli package.

## Goal
Provide users with flexibility in how chat messages are visually separated and how sender labels are displayed relative to message content:
- Control whether an empty line is inserted between consecutive messages.
- Control whether sender and content appear on the same line or on separate lines.

## Acceptance Criteria

- Introduce one new config flags under the UI section:
  - `message_spacing: true|false` controls inserting a blank line between messages when true.
- default to `false` to preserve current compact layout.
- When `message_spacing` is enabled, render an empty line between each message bubble or block.
- Add unit tests to verify the layout produces the correct sequence of lines.

## Implementation

**How it was implemented**  
- Extend the chat UI renderer to read `message_spacing` from config.
- In the message rendering routine, after emitting each message block, conditionally insert a blank line if `message_spacing` is true.
- Write unit tests for values of `(message_spacing)` covering single-line messages, multi-line content, and boundaries.

## Notes

- These options improve readability for users who prefer more visual separation or clearer sender labels.
- Keep default settings unchanged to avoid surprising existing users.
