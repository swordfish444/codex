+++
id = "23"
title = "Interactive Container Command Affordance via Hotkey"
status = "Done"
dependencies = "01" # Rationale: depends on Task 01 for mount-add/remove affordance
last_updated = "2025-06-30T12:00:00.000001"
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

**Planned implementation steps**
- Define a new slash command `Shell` and dispatch it in `app.rs` to push an interactive shell prompt.
- Bind `Ctrl+M` in `ChatComposer` to toggle shell-command mode and invoke the shell prompt.
- Create `ShellCommandView` (a bottom-pane overlay) to capture arbitrary shell input and emit `AppEvent::ShellCommand(cmd)`.
- Use existing `AppEvent::ShellCommand` and `ShellCommandResult` variants to handle invocation and results.
- Implement `ChatWidget::handle_shell_command` to execute `sh -c <cmd>` asynchronously and record the execution in conversation history.
- Implement `ChatWidget::handle_shell_command_result` and extend conversation rendering to display command outputs inline.
- Add unit and integration tests to verify hotkey binding, prompt display, event emission, and inline rendering of output.

## Notes

- This feature aids debugging and inspection without leaving the agent workflow.
- Ensure that security policies (e.g. sandbox restrictions) still apply to these commands.
