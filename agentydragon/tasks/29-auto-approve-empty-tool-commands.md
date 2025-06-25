---
id: 29
title: Auto-Approve Empty-Array Tool Invocations
status: Not started  # one of: Not started, Started, Needs manual review, Done, Cancelled
dependencies: "03,06,08,13,15,32,18,19,22,23"
summary: Automatically approve tool-use requests where the command array is empty, bypassing the approval prompt.
goal: |
  In rare cases the model may emit a tool invocation event with an empty `command: []`.  These invocations cannot succeed and continually trigger errors.  Automatically treat empty-array tool requests as approved (once), suppressing the approval UI, to allow downstream error handling rather than perpetual prompts.

## Acceptance Criteria

- Detect tool requests where `command: []` (no arguments).
- Do not open the approval prompt for these cases; instead, automatically approve and allow the tool pipeline to proceed (and eventually handle the error).
- Include a unit test simulating an empty-array tool invocation that verifies no approval prompt is shown and that a `ReviewDecision::Approved` is returned immediately.

## Implementation

**How it was implemented**  
- In the command-review widget setup (`ApprovalRequest::Exec`), check for `command.is_empty()` before rendering; if empty, directly send `ReviewDecision::Approved` and mark the widget done.
- Add a Rust unit test for `UserApprovalWidget` to feed an `Exec { command: vec![] }` request and assert automatic approval without rendering the select mode.

## Notes

- This is a pragmatic workaround for spurious empty‑command tool calls; a more robust model‑side fix may replace this later.
