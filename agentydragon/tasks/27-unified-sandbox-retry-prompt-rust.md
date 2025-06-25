+++
id = "27"
title = "Unified Sandbox-Retry Prompt with y/a/A/n Options (Rust)"
status = "Not started"
dependencies = "03,06,08,13,15,32,18,19,22,23"
last_updated = "2025-06-25T01:40:09.600000"
+++

## Summary
Implement a unified retry‑without‑sandbox prompt in the Rust TUI with one‑shot, session‑scoped, and persistent options.

## Goal
Replace the two-stage sandbox‑retry and approval flow with a single, unified prompt in the Rust UI.  Provide four hotkey options (y/a/A/n) to control sandbox behavior at varying scopes:
- y: retry this one command without sandbox
- a: always run without sandbox but still ask first
- A: always run without sandbox and never ask again
- n: keep using sandbox

## Acceptance Criteria

- When a sandboxed shell invocation fails (exit code ≠ 0), display a single prompt:
  ```
  Retry without sandbox

    y Yes, run without sandbox this one time
    a Yes, always run without sandbox but still ask me first
    A Yes, always run without sandbox and do not ask again
    n No, keep using sandbox
  ```
- Hotkeys y/a/A/n must map to the corresponding behavior and dismiss the prompt.
- The prompt replaces the older two‑stage “retry?” + “Allow command?” dialogs.
- Add unit/integration tests simulating a failing sandbox command and each hotkey path, verifying correct sandbox flag logic.

## Implementation

**How it was implemented**  
- Refactor the sandbox error handler in `tui/src/shell.rs` to emit a single `SandboxRetryPrompt` event instead of separate prompts.
- Create a new TUI widget `SandboxRetryWidget` that renders the four-line menu and captures y/a/A/n keys.
- Map each choice to updating the per-session config (`Config.tui.sandbox_mode`) and retrying or aborting the command as appropriate.
- Update the shell‑invocation pipeline to consult the new `sandbox_mode` setting and skip sandbox when indicated.
- Write Rust tests (in `tui/tests/`) to simulate sandbox failures and user key presses for all four options.

## Notes

- This unifies and simplifies the UX, removing confusion from layered prompts.
- The three levels of scope (one-off, scoped prompt, no prompt) give power users flexibility and safety.
