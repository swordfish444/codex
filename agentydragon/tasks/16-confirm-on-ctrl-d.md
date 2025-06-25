+++
id = "16"
title = "Confirm on Ctrl+D to Exit"
status = "Not started"
dependencies = "" # No prerequisites
last_updated = "2025-06-25T01:40:09.513723"
+++

# Task 16: Confirm on Ctrl+D to Exit

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal

Require two consecutive Ctrl+D keystrokes (within a short timeout) to exit the TUI, preventing accidental termination from a single SIGINT.

## Acceptance Criteria

- Add a `[tui] require_double_ctrl_d = true` config flag (default `false`) to enable double‑Ctrl+D exit confirmation.
- When `require_double_ctrl_d` is enabled:
  - First Ctrl+D within the TUI suspends exit and shows a status message like "Press Ctrl+D again to confirm exit".
  - If a second Ctrl+D occurs within a configurable timeout (e.g. 2 sec), the TUI exits normally.
  - If no second Ctrl+D arrives before timeout, clear the confirmation state and resume normal operation.
- Ensure that child processes (shell tool calls) still receive SIGINT immediately and are not affected by the double‑Ctrl+D logic.
- Prevent immediate exit on Ctrl+D (EOF); require the same double‑confirmation workflow as for Ctrl+D when EOF is received.
- Provide unit or integration tests simulating SIGINT events to verify behavior.

## Implementation

**How it was implemented**  
- Introduce `require_double_ctrl_d: bool` in `ConfigToml` → `Config` under the `tui` section, with default `false`.
- Extend the TUI event loop (e.g. in `tui/src/app.rs`) to handle SIGINT events:
  1. If `require_double_ctrl_d` is disabled, behave as before (exit on first Ctrl+D).
  2. If enabled and not already confirming, enter a `ConfirmExit` state, record timestamp, and display confirmation message.
  3. If enabled and in `ConfirmExit` state, exit immediately on second Ctrl+D.
  4. On each TUI tick, if in `ConfirmExit` and timeout elapsed, clear `ConfirmExit` state.
  5. Intercept EOF (Ctrl+D) events in the input handler and apply the same `ConfirmExit` logic as for Ctrl+D when `require_double_ctrl_d` is enabled.
- Add rendering logic in the status bar (`tui/src/status_indicator_widget.rs` or similar) to show the confirmation prompt.

**How it works**  
- On startup, the TUI reads `require_double_ctrl_d` from config.
- When SIGINT is captured by the event loop, double‑Ctrl+D logic intercepts and requires confirmation.
- Child processes continue to get raw SIGINT from the OS because the TUI should delegate signals while awaiting child termination.

## Notes

- Make the double‑Ctrl+D timeout duration configurable if desired (e.g. via `tui.double_ctrl_d_timeout_secs`).
- Ensure that existing tests for Ctrl+D behavior are updated or new tests added to cover the confirmation state.
