+++
id = "16"
title = "Confirm on Ctrl+D to Exit"
status = "Merged"
dependencies = ""
last_updated = "2025-06-25T05:36:23.493497"
+++

# Task 16: Confirm on Ctrl+D to Exit

> *This task is specific to codex-rs.*

## Status

**General Status**: Done  
**Summary**: Double Ctrl+D confirmation implemented and tested.

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
- Added `require_double_ctrl_d` and `double_ctrl_d_timeout_secs` to the TUI config in `core/src/config_types.rs` with defaults.
- Introduced `ConfirmCtrlD` helper in `tui/src/confirm_ctrl_d.rs` to manage confirmation state and expiration logic.
- Extended `App` in `tui/src/app.rs`:
  - Initialized `confirm_ctrl_d` from config in `App::new`.
  - Expired stale confirmation windows each event-loop tick and cleared the status overlay when timed out.
  - Replaced the Ctrl+D handler to invoke `ConfirmCtrlD::handle`, exiting only on confirmed press and otherwise displaying a prompt via `BottomPane`.
- Leveraged `BottomPane::set_task_running(true)` and `update_status_text` to render the confirmation prompt overlay.
- Added unit tests for `ConfirmCtrlD` in `tui/src/confirm_ctrl_d.rs` covering disabled mode, confirmation press, and timeout expiration.

**How it works**  
- When `require_double_ctrl_d = true`, the first Ctrl+D press shows "Press Ctrl+D again to confirm exit" in the status overlay.
- A second Ctrl+D within `double_ctrl_d_timeout_secs` exits the TUI; otherwise the prompt and state clear after timeout.
- When `require_double_ctrl_d = false`, Ctrl+D exits immediately as before.
- Child processes still receive SIGINT normally since only the TUI event loop intercepts Ctrl+D.

## Notes

- Make the double‑Ctrl+D timeout duration configurable if desired (e.g. via `tui.double_ctrl_d_timeout_secs`).
- Ensure that existing tests for Ctrl+D behavior are updated or new tests added to cover the confirmation state.
