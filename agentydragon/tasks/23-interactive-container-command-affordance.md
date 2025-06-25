+++
id = "23"
title = "Interactive Container Command Affordance via Hotkey"
status = "Not started"
dependencies = "01" # Rationale: depends on Task 01 for mount-add/remove affordance
last_updated = "2025-06-25T01:40:09.600000"
+++

## Summary
Provide a keybinding to run arbitrary shell commands in the agent’s container and display output inline.

## Goal
Add a user-facing affordance (e.g. a hotkey) to invoke arbitrary shell commands within the agent's container during a session for on-demand inspection and debugging.  The typed command should be captured as a chat turn, executed via the existing shell tool, and its output rendered inline in the chat UI.

## Acceptance Criteria

- Bind a hotkey (e.g. Ctrl+M) that opens a prompt for the user to type any shell command.
- When the user submits, capture the command as if entered in the chat input, and invoke the shell tool with the command in the agent’s container.
- Display the command invocation and its stdout/stderr output inline in the chat window, respecting formatting rules (e.g. compact rendering settings).
- Support chaining multiple commands in separate turns; history should show these command turns normally.
- Provide unit or integration tests simulating a user hotkey press, command input, and verifying the shell tool is called and output is displayed.

## Implementation

**How it was implemented**  
- Define a new keybinding (configurable, default Ctrl+M) in the TUI to trigger a `ShellCommandPrompt` overlay.
- In the overlay, accept arbitrary user input and dispatch it as a `ToolInvocation(ShellTool, command)` event in the agent’s event loop.
- Leverage the existing shell tool backend to execute the command in the container and capture its output.
- Render the command invocation and result inline in the chat UI using the command-rendering logic (honoring compact mode and spacing options).
- Add integration tests to simulate the hotkey, input prompt, and verify the shell tool call and inline rendering.

## Notes

- This feature aids debugging and inspection without leaving the agent workflow.
- Ensure that security policies (e.g. sandbox restrictions) still apply to these commands.
