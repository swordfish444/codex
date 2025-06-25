+++
id = "36"
title = "Add Tests for Interactive Prompting While Executing"
status = "Not started"
dependencies = "13"
last_updated = "2025-06-25T04:45:29Z"
+++

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Follow-up to Task 13; add unit tests for interactive prompt overlay during execution.

## Goal

Write tests that verify `BottomPane::handle_key_event` forwards input to the composer while `is_task_running`, preserving the status overlay until completion.

## Acceptance Criteria

- Unit tests covering key events (e.g. alphanumeric, Enter) during `is_task_running == true`.
- Assertions that `active_view` remains a `StatusIndicatorView` while running and is removed when `set_task_running(false)` is called.
- Coverage of redraw requests and correct `InputResult` values.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Refer to existing tests in `user_approval_widget.rs` and `set_title_view.rs` for testing patterns.
