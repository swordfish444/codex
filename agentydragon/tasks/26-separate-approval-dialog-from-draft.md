---
id: 26
title: Render Approval Requests in Separate Dialog from Draft Window
status: Not started  # one of: Not started, Started, Needs manual review, Done, Cancelled
summary: Display patch approval prompts in a distinct dialog or panel to avoid overlaying the draft editor.
goal: |
  Change the chat UI so that approval requests (patch diffs for approve/deny) appear in a separate dialog element or panel, positioned adjacent to or below the chat window, rather than overlaying the draft input area.
  This eliminates overlay conflicts and ensures the draft editor remains fully visible and interactive while reviewing patches.

## Acceptance Criteria

- Approval prompts with diffs open in a distinct UI element (e.g. side panel or bottom pane) that does not obscure the draft editor.
- The draft input area remains fully visible and editable whenever an approval dialog is active.
- The approval dialog is visually distinguished (border, background) and clearly labeled.
- The layout adjusts responsively for narrow/short terminal sizes, maintaining separation without clipping content.
- Add functional tests or integration tests verifying that the draft input remains accessible and that the approval dialog contents are rendered in the new panel.

## Implementation

**How it was implemented**  
- Refactor the patch-approval renderer to spawn a separate TUI view (`ApprovalDialogView`) instead of the overlay popup.
- Allocate a consistent panel region (e.g. bottom X rows or right-hand column) for approval dialogs, reserving the draft editor region above or to the left.
- Update layout logic to recalculate positions on terminal resize, ensuring both panels remain visible.
- Style the new dialog with its own borders and title bar (e.g. "Approval Request").
- Add integration tests using the TUI test harness to simulate opening approval prompts and verifying that typing in the draft area still works and that the dialog appears in the correct panel.

## Notes

- This change fixes the long-standing overlay bug where approval diffs obstruct the draft.  
- Future enhancements may allow toggling between inline overlay or separate panel modes.
