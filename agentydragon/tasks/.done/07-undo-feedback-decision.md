+++
id = "07"
title = "Undo Feedback Decision with Esc Key"
status = "Merged"
dependencies = "01,04,10,12,16,17"
last_updated = "2025-06-25T01:40:09.506146"
+++

# Task 07: Undo Feedback Decision with Esc Key

> *This task is specific to codex-rs.*

## Status

**General Status**: Merged  
**Summary**: ESC key now cancels feedback entry and returns to the select menu, preserving any entered text; implementation and tests added.

## Goal
Enhance the user-approval dialog so that if the user opted to leave feedback (“No, enter feedback”) they can press `Esc` to cancel the feedback flow and return to the previous approval choice menu (e.g. “Yes, proceed” vs. “No, enter feedback”).

## Acceptance Criteria
- While the feedback-entry textarea is active, pressing `Esc` closes the feedback editor and reopens the yes/no confirmation dialog.
- The cancellation must restore the dialog state without losing any partially entered feedback text.

## Implementation

**How it was implemented**  
- In `tui/src/user_approval_widget.rs`, updated `UserApprovalWidget::handle_input_key` so that pressing `Esc` in input mode switches `mode` back to `Select` (rather than sending a deny decision), and restores `selected_option` to the feedback entry item without clearing the input buffer.
- Added a unit test in the same module to verify that `Esc` cancels input mode, preserves the feedback text, and does not emit any decision event.

**How it works**  
- When the widget is in `Mode::Input` (feedback-entry), receiving `KeyCode::Esc` resets `mode` to `Select` and sets `selected_option` to the index of the “Edit or give feedback” option.  
- The `input` buffer remains intact, so any partially typed feedback is preserved for if/when the user re-enters feedback mode.  
- No approval decision is sent on `Esc`, so the modal remains active and the user can still approve, deny, or re-enter feedback.

## Notes
- Changes in `tui/src/user_approval_widget.rs` to treat `Esc` in input mode as a cancel-feedback action and added corresponding tests.
