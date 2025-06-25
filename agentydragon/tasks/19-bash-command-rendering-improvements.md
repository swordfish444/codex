+++
id = "19"
title = "Bash Command Rendering Improvements for Less Verbosity"
status = "In progress"
dependencies = "02,07,09,11,14,29"
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
This change will touch both the event-processing and rendering layers of the Rust TUI:

- **Event processing** (`codex-rs/exec/src/event_processor.rs`):
  - Strip any `bash -lc` wrapper when formatting shell commands via `escape_command`.
  - Replace verbose `BackgroundEvent` logs for sandbox-denied errors and automatic retries with a unified exec-command begin/end sequence.
  - Annotate completed commands with either a checkmark (✅) and `<duration>ms` for success or `exit code: N <duration>ms` for failures.

- **TUI rendering** (`codex-rs/tui/src/history_cell.rs`):
  - Collapse consecutive `BackgroundEvent` entries related to exec failures/retries into the standard active/completed exec-command cells.
  - Update `new_active_exec_command` and `new_completed_exec_command` to use the new inline format (icon or exit code + duration, with `$ <command>` on the same block).
  - Ensure role labels and plain-text messages render on a single line separated by a space.

- **Tests** (`codex-rs/tui/tests/`):
  - Add or update test fixtures to verify:
    - Commands appear without any `bash -lc` boilerplate.
    - Completed commands show the correct checkmark or exit-code annotation with accurate duration formatting.
    - Background debugging events no longer leak raw debug strings and are correctly collapsed into the exec-command flow.

## Notes

- Improves readability of interactive sessions and logs by reducing boilerplate.
- Ensure compatibility with both live TUI output and persisted log transcripts.
