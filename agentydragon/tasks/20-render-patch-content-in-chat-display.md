---
id: 20
title: Render Patch Content in Chat Display Window for Approve/Deny
status: Not started  # one of: Not started, Started, Needs manual review, Done, Cancelled
summary: Improve inline display of patch hunks in chat messages for approval workflows.
goal: |
  Adjust the chat UI so that when the assistant proposes patches for approval or denial:
  - The current patch being queried is shown in full, with file paths relative to the CWD (or absolute if outside CWD).
  - Previous patches collapse to a configurable number of context lines (e.g. first and last X lines).
  - Omit verbose event logs (e.g. `PatchApplyEnd(PatchApplyEndEvent { ... })`), replacing them with concise annotations like "patch applied".
  - Maintain clear separation between patches and conversational messages.
---
> *This task is specific to the chat UI renderer.*

## Acceptance Criteria

- When displaying a patch for approve/deny, the full diff for the active patch is rendered inline in the chat window.
- Older or superseded patches collapse to show only up to N lines of context, with an indicator (e.g. "... 10 lines collapsed ...").
- File paths in diff headers are shown relative to the current working directory, unless the file resides outside the CWD.
- Event logs around patch application are simplified: drop structured event data and replace with a simple status note (e.g. "patch applied").
- Configurable parameter (e.g. `patch_context_lines`) controls the number of context lines for collapsed hunks.
- Preserve the user’s draft input when an approval dialog or patch diff appears; ensure the draft editor remains visible so users can continue editing while reviewing.
- Provide end-to-end integration tests that simulate drafting long messages, triggering approval dialogs and overlays, and verify that all UI elements (draft editor, diffs, logs) render correctly without overlap or content loss.

## Implementation

**How it was implemented**  
- Extend the chat renderer to detect patch approval prompts and render diffs using a custom formatter.
- Compute relative paths via `Path::strip_prefix`, falling back to full path if outside CWD.
- Track the current patch ID and render its full content; collapse previous patch bodies according to `patch_context_lines` setting.
- Preserve and render the current draft buffer alongside the active patch diff, ensuring live edits remain visible during approval steps.
- Add integration tests using the TUI test harness or end-to-end framework to simulate user input of long text, approval flows, overlay dialogs, and log output, asserting correct screen layout and content integrity.
- Replace verbose event debug output with a single-line status message.

## Notes

- Users can override `patch_context_lines` in their config to see more or fewer collapsed lines.
- Ensure compatibility with both live TUI sessions and persisted transcript logs.
