+++
id = "19"
title = "Bash Command Rendering Improvements for Less Verbosity"
status = "Not started"
dependencies = "" # No prerequisites
last_updated = "2025-06-25T01:40:09.600000"
+++

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
