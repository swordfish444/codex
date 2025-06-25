+++
id = "31"
title = "Display Remaining Context Percentage in codex-rs TUI"
status = "Not started"
dependencies = "03,06,08,13,15,32,18,19,22,23"
last_updated = "2025-06-25T01:40:09.600000"
+++

## Summary
Show a live "x% context left" indicator in the TUI (Rust) to inform users of remaining model context buffer.

## Goal
Enhance the codex-rs TUI by adding a status indicator that displays the percentage of model context buffer remaining (e.g. "75% context left").  Update this indicator dynamically as the conversation progresses.

## Acceptance Criteria

- Compute current token usage and total context limit from the active session.
- Display "<N>% context left" in the status bar or header of the TUI, formatted compactly.
- Update the percentage after each message turn in real time.
- Ensure the indicator is visible but does not obstruct existing UI elements.
- Add unit or integration tests mocking token count updates and verifying correct percentage formatting (rounding behavior, boundary conditions).

## Implementation

**How it was implemented**  
- Added a `history_items: Vec<ResponseItem>` field to `ChatWidget` to accumulate the raw sequence of messages and function calls.
- Created a new module `tui/src/context.rs` mirroring the JS heuristics:
  - `approximate_tokens_used(&[ResponseItem])`: counts characters in text and function-call items, divides by 4 and rounds up.
  - `max_tokens_for_model(&str)`: uses a registry of known model limits and heuristic fallbacks (32k, 16k, 8k, 4k, default 128k).
  - `calculate_context_percent_remaining(&[ResponseItem], &str)`: computes `(remaining / max) * 100`.
- Updated `ChatWidget::replay_items` and `ChatWidget::handle_codex_event` to push each incoming `ResponseItem` into `history_items`.
- Modified `ChatComposer::render_ref` to query `calculate_context_percent_remaining`, format and display "<N>% context left" after the input area, coloring it green/yellow/red per thresholds (>40%, 25–40%, ≤25%).
- Added unit tests in `tui/tests/context_percent.rs` covering token counting, model heuristics, percent rounding, and boundary conditions.

## Notes

- This feature helps users anticipate when they may need to truncate history or start a new session.
- Future enhancement: allow toggling this indicator on/off via config.
