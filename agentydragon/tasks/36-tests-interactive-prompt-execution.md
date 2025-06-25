+++
id = "36"
title = "Add Tests for Interactive Prompting While Executing"
status = "Done"
dependencies = "06,13" # Rationale: depends on Tasks 06 and 13 for external editor and interactive prompt support
last_updated = "2025-06-25T11:05:55Z"
+++

> *This task is specific to codex-rs.*

## Status

**General Status**: Done  
**Summary**: Follow-up to Task 13; add unit tests for interactive prompt overlay during execution.

## Goal

Write tests that verify `BottomPane::handle_key_event` forwards input to the composer while `is_task_running`, preserving the status overlay until completion.

## Acceptance Criteria

- Unit tests covering key events (e.g. alphanumeric, Enter) during `is_task_running == true`.
- Assertions that `active_view` remains a `StatusIndicatorView` while running and is removed when `set_task_running(false)` is called.
- Coverage of redraw requests and correct `InputResult` values.

## Implementation

**Planned Approach**

- Use existing `make_pane` and `make_pane_and_rx` helpers to create a `BottomPane` in a running-task state.
- Write unit tests in `tui/src/bottom_pane/mod.rs` that verify:
  - Typing alphanumeric characters while `is_task_running == true` appends to the composer, maintains the `StatusIndicatorView` overlay, and emits a `AppEvent::Redraw`.
  - Pressing Enter returns `InputResult::Submitted` with the buffered text, clears the composer, retains the overlay, and triggers a redraw.
  - Calling `set_task_running(false)` removes the status indicator overlay.
- Follow existing patterns from the tests in `user_approval_widget.rs` and `set_title_view.rs`.

## Notes

- Refer to existing tests in `user_approval_widget.rs` and `set_title_view.rs` for testing patterns.
