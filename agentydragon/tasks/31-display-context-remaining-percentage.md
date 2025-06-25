---
id: 31
title: Display Remaining Context Percentage in codex-rs TUI
status: Not started  # one of: Not started, Started, Needs manual review, Done, Cancelled
dependencies: "03,06,08,13,15,32,18,19,22,23"
summary: Show a live "x% context left" indicator in the TUI (Rust) to inform users of remaining model context buffer.
goal: |
  Enhance the codex-rs TUI by adding a status indicator that displays the percentage of model context buffer remaining (e.g. "75% context left").  Update this indicator dynamically as the conversation progresses.

## Acceptance Criteria

- Compute current token usage and total context limit from the active session.
- Display "<N>% context left" in the status bar or header of the TUI, formatted compactly.
- Update the percentage after each message turn in real time.
- Ensure the indicator is visible but does not obstruct existing UI elements.
- Add unit or integration tests mocking token count updates and verifying correct percentage formatting (rounding behavior, boundary conditions).

## Implementation

**How it was implemented**  
- Extend the session state in `tui/src/app.rs` or relevant module to track token usage and context limit.
- After each send/receive event, recalculate `remaining = (limit - used) * 100 / limit`.
- Render the indicator via the status bar widget (`tui/src/status_indicator_widget.rs`), appending `"{remaining}% context left"`.
- Add tests in `tui/tests/` that simulate message additions and assert the status rendering shows correct percentages at key usage points.

## Notes

- This feature helps users anticipate when they may need to truncate history or start a new session.
- Future enhancement: allow toggling this indicator on/off via config.
