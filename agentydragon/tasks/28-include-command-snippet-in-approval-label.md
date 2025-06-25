+++
id = "28"
title = "Include Command Snippet in Session-Scoped Approval Label"
status = "Not started"
dependencies = "03,06,08,13,15,32,18,19,22,23"
last_updated = "2025-06-25T01:40:09.600000"
+++

## Summary
When asking for session-scoped approval of a command, embed a truncated snippet of the actual command in the approval label for clarity.

## Goal
Improve the session-scoped approval option label for commands by including a backtick-quoted snippet of the command itself (truncated to fit).  This makes it clear exactly which command (including parameters) will be auto-approved for the session.

## Acceptance Criteria

- The session-scoped approval label changes from generic text to include a snippet of the current command, e.g.:  
  ```text
  Yes, always allow running `cat x | foo --bar > out` for this session (a)
  ```
- If the command is too long, truncate the middle (e.g. `long-part…end-part`) to fit a configurable max length.
- Implement the snippet templating in both Rust and JS UIs for consistency.
- Add unit tests to verify snippet extraction, truncation logic, and label rendering for various command lengths.

## Implementation

**How it was implemented**  
- In the command-review widget, capture the `commandForDisplay` string and apply a `truncateMiddle(maxLen)` helper.
- Embed the truncated snippet into the session-scoped approval option label.
- Make `maxSnippetLength` configurable via UI settings (default e.g. 30 characters).
- Add tests covering snippet lengths under, equal to, and exceeding the max length, verifying correct ellipsis placement.

## Notes

- This clarifies what parameters will be auto-approved and avoids ambiguity when multiple similar commands occur.
