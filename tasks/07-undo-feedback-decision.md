# Task 07: Undo Feedback Decision with Esc Key

## Goal
Enhance the user-approval dialog so that if the user opted to leave feedback (“No, enter feedback”) they can press `Esc` to cancel the feedback flow and return to the previous approval choice menu (e.g. “Yes, proceed” vs. “No, enter feedback”).

## Acceptance Criteria
- While the feedback-entry textarea is active, pressing `Esc` closes the feedback editor and reopens the yes/no confirmation dialog.
- The cancellation must restore the dialog state without losing any partially entered feedback text.

## Notes
- Changes in `tui/src/bottom_pane/approval_modal_view.rs` and input handling in the approval modal.