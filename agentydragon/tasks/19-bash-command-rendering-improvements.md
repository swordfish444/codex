---
id: 19
title: Bash Command Rendering Improvements for Less Verbosity
status: Not started
summary: Render bash commands more concisely and shorten tool output annotations.
goal: |
  Adjust agent output formatting for shell commands and tool logs to:
  - Render bash commands without wrapping them in `bash -lc "..."`.
  - Display role-prefixed messages on a single line (role and content space-separated).
  - Shorten command-result annotations, using `✅ <duration>ms` for successful runs or `exit code: <code> <duration>ms` for failures.

> *This task is specific to per-agent UI conventions and log readability.*

## Acceptance Criteria

- Shell commands render as plain text without `bash -lc` wrappers.
- Role labels and message content appear on the same line, separated by a space.
- Command-result annotations show a checkmark and duration for zero exit codes, or `exit code: N` and duration for nonzero codes, in the format `<icon or exit code> <duration>ms`.
- Existing functionality remains unaffected beyond formatting changes.
- Verbose background event logs (e.g. sandbox‑denied exec errors, retries) collapse into a single command execution entry showing command start, running indicator, and concise completion status.
- Automated examples or tests verify the new rendering behavior.

## Implementation

**How it was implemented**  
- Update the internal shell-command renderer to strip out `bash -lc` wrappers and emit raw commands.
- Modify the message formatting component to place role labels and content on one line.
- Refactor the result-annotation logic to emit `✅ 12ms` or `exit code: 123 12ms`.
- Add or extend tests/examples to cover these formatting rules.

## Notes

- Improves readability of interactive sessions and logs by reducing boilerplate.
- Ensure compatibility with both live TUI output and persisted log transcripts.
