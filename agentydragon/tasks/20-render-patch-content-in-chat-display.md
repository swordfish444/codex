+++
id = "20"
title = "Render Patch Content in Chat Display Window for Approve/Deny"
status = "Not started"
dependencies = ""
last_updated = "2025-06-25T01:41:34.738344"
+++

> *This task is specific to the chat UI renderer.*

## Acceptance Criteria

- When displaying a patch for approve/deny, the full diff for the active patch is rendered inline in the chat window.
- Older or superseded patches collapse to show only up to N lines of context, with an indicator (e.g. "... 10 lines collapsed ...").
- File paths in diff headers are shown relative to the current working directory, unless the file resides outside the CWD.
- Event logs around patch application are simplified: drop structured event data and replace with a simple status note (e.g. "patch applied").
- Configurable parameter (e.g. `patch_context_lines`) controls the number of context lines for collapsed hunks.
- Preserve the user’s draft input when an approval dialog or patch diff appears; ensure the draft editor remains visible so users can continue editing while reviewing.
- Provide end-to-end integration tests that simulate drafting long messages, triggering approval dialogs and overlays, and verify that all UI elements (draft editor, diffs, logs) render correctly without overlap or content loss.
- Exhaustively test all dialog interaction flows (approve, deny, cancel) and overlay scenarios to confirm consistent behavior across combinations and prevent rendering artifacts.

## Implementation

**How it was implemented**  
- Extend the chat renderer to detect patch approval prompts and render diffs using a custom formatter.
- Compute relative paths via `Path::strip_prefix`, falling back to full path if outside CWD.
- Track the current patch ID and render its full content; collapse previous patch bodies according to `patch_context_lines` setting.
- Preserve and render the current draft buffer alongside the active patch diff, ensuring live edits remain visible during approval steps.
- Add integration tests using the TUI test harness or end-to-end framework to simulate user input of long text, approval flows, overlay dialogs, and log output, asserting correct screen layout and content integrity.
- Design a parameterized test matrix covering all dialog interaction flows (approve/deny/cancel) and overlay transitions to ensure exhaustive coverage and UI sanity.
- Replace verbose event debug output with a single-line status message.

## Notes

- Users can override `patch_context_lines` in their config to see more or fewer collapsed lines.
- Ensure compatibility with both live TUI sessions and persisted transcript logs.