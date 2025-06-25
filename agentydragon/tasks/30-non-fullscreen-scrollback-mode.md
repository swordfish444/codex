---
id: 30
title: Non-Fullscreen Scrollback Mode with Native Terminal Scroll
status: Not started  # one of: Not started, Started, Needs manual review, Done, Cancelled
dependencies: "03,06,08,13,15,32,18,19,22,23"
summary: Offer a non-fullscreen TUI mode that appends conversation output and defers scrolling to the terminal scrollback.
goal: |
  Provide an optional non-fullscreen mode for the chat UI where:
  - The TUI does not capture the mouse scroll wheel.
  - All conversation output is appended in place, allowing the terminal's native scrollback to navigate history.
  - The user-entry window remains fixed at the bottom of the terminal.
  - The entire UI runs in a standard terminal buffer (no alternate screen), so the user can use their terminal’s scrollbar or scrollback keys to review past messages.

## Acceptance Criteria

- Introduce a `tui.non_fullscreen_mode` config flag (default `false`).
- When enabled, the application:
  - Disables alternate screen buffering (i.e. does not switch to the TUI alt-screen).
  - Does not intercept mouse scroll events; scroll events are passed through to the terminal.
  - Renders new chat messages inline (appended) rather than redrawing the full viewport.
  - Keeps the user input prompt visible at the bottom after each message.
- Add integration tests or manual validation steps to confirm that: scrollback keys/mouse scroll work via terminal scrollback, and the prompt remains in view.

## Implementation

**How it was implemented**  
- Add `non_fullscreen_mode: bool` to the `tui` config section.
- In the TUI initialization, skip entering the alternate screen and disable pannable viewports.
- Remove mouse event capture for scroll wheel events when `non_fullscreen_mode` is true.
- Change rendering loop: after each new message, print the message directly to the stdout buffer (in append mode), then redraw only the input prompt line.
- Write integration tests that spawn the TUI in non-fullscreen mode, emit multiple messages, send scroll events (if possible), and assert that scrollback buffer contains the messages.

## Notes

- This mode trades advanced in-TUI scrolling features for simplicity and compatibility with users’ accustomed terminal scrollback.
- It may not support complex viewport resizing; documentation should note that.
