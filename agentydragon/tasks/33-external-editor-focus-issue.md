+++
id = "33"
title = "Fix External Editor Focus Issue"
status = "Not started"
summary = "When launching the external editor from the TUI (e.g. nvim), keyboard input is still captured by the Rust TUI, causing keys to split between the editor and the TUI."
dependencies = "06"
last_updated = "2025-06-25T01:40:09.700000"
+++

# Task 33: Fix External Editor Focus Issue

## Goal
Ensure that when the TUI spawns an external editor, it fully hands off keyboard control to the editor, and upon editor exit, restores TUI input handling without leaking keystrokes or misrouting commands.

## Acceptance Criteria

- Launching external editor via `/edit-prompt` or Ctrl+E disables TUI raw mode and event capture so all keystrokes go directly to the editor.
- Upon editor exit, raw mode and event capture are correctly re-enabled, and no keystrokes are lost or misrouted.
- No residual input events are processed by the TUI while the editor is running.
- Add integration tests or manual validation steps simulating editor launch and exit sequences.

## Implementation

**High-level plan**  
- Before spawning the editor process (in `ChatComposer`), call `disable_raw_mode()` and `disable_event_capture()` to restore normal terminal behavior.  
- Spawn the editor subprocess and wait for it to exit.  
- After exit, re-enable raw mode and event capture via `enable_raw_mode()` and `enable_event_capture()`.  
- Wrap this sequence in a helper function (e.g., `spawn_external_editor`) and update the `/edit-prompt` handler to use it.
- Add integration tests in `tui/tests/` that mock the editor command (e.g., `echo`) to verify terminal mode transitions.

## Notes

- Use Crossterm APIs for terminal mode management.  
- Ensure interruption signals (e.g., Ctrl+C) during editor sessions are propagated correctly to avoid TUI deadlock.
