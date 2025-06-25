+++
id = "16"
title = "Confirm on Ctrl+C to Exit"
status = "Not started"
dependencies = ""
last_updated = "2025-06-25T01:40:09.513723"
+++

# Task 16: Confirm on Ctrl+C to Exit

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal

Require two consecutive Ctrl+C keystrokes (within a short timeout) to exit the TUI, preventing accidental termination from a single SIGINT.

## Acceptance Criteria

- Add a `[tui] require_double_ctrl_c = true` config flag (default `false`) to enable double‑Ctrl+C exit confirmation.
- When `require_double_ctrl_c` is enabled:
  - First Ctrl+C within the TUI suspends exit and shows a status message like "Press Ctrl+C again to confirm exit".
  - If a second Ctrl+C occurs within a configurable timeout (e.g. 2 sec), the TUI exits normally.
  - If no second Ctrl+C arrives before timeout, clear the confirmation state and resume normal operation.
- Ensure that child processes (shell tool calls) still receive SIGINT immediately and are not affected by the double‑Ctrl+C logic.
- Prevent immediate exit on Ctrl+D (EOF); require the same double‑confirmation workflow as for Ctrl+C when EOF is received.
- Provide unit or integration tests simulating SIGINT events to verify behavior.

## Implementation

**How it was implemented**  
- Introduce `require_double_ctrl_c: bool` in `ConfigToml` → `Config` under the `tui` section, with default `false`.
- Extend the TUI event loop (e.g. in `tui/src/app.rs`) to handle SIGINT events:
  1. If `require_double_ctrl_c` is disabled, behave as before (exit on first Ctrl+C).
  2. If enabled and not already confirming, enter a `ConfirmExit` state, record timestamp, and display confirmation message.
  3. If enabled and in `ConfirmExit` state, exit immediately on second Ctrl+C.
  4. On each TUI tick, if in `ConfirmExit` and timeout elapsed, clear `ConfirmExit` state.
  5. Intercept EOF (Ctrl+D) events in the input handler and apply the same `ConfirmExit` logic as for Ctrl+C when `require_double_ctrl_c` is enabled.
- Add rendering logic in the status bar (`tui/src/status_indicator_widget.rs` or similar) to show the confirmation prompt.

**How it works**  
- On startup, the TUI reads `require_double_ctrl_c` from config.
- When SIGINT is captured by the event loop, double‑Ctrl+C logic intercepts and requires confirmation.
- Child processes continue to get raw SIGINT from the OS because the TUI should delegate signals while awaiting child termination.

## Notes

- Make the double‑Ctrl+C timeout duration configurable if desired (e.g. via `tui.double_ctrl_c_timeout_secs`).
- Ensure that existing tests for Ctrl+C behavior are updated or new tests added to cover the confirmation state.
