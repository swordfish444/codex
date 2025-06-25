+++
id = "23"
title = "Interactive Container Command Affordance via Hotkey"
status = "Done"
dependencies = "01" # Rationale: depends on Task 01 for mount-add/remove affordance
last_updated = "2025-06-26T15:00:00.000000"
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
- Added a new slash command `Shell` and updated dispatch logic in `app.rs` to push a shell-command view.
- Bound `Ctrl+M` in `ChatComposer` to dispatch `SlashCommand::Shell` for hotkey-driven shell prompt.
- Created `ShellCommandView` (bottom pane overlay) to capture arbitrary user input and emit `AppEvent::ShellCommand(cmd)`.
- Extended `AppEvent` with `ShellCommand(String)` and `ShellCommandResult { call_id, stdout, stderr, exit_code }` variants for round-trip messaging.
- Implemented `ChatWidget::handle_shell_command` to execute `sh -c <cmd>` asynchronously (tokio::spawn) and send back `ShellCommandResult`.
- Updated `ConversationHistoryWidget` to reuse existing exec-command cells to display shell commands and their output inline.
- Added tests:
  - Unit test in `shell_command_view.rs` asserting correct event emission (skipping redraws).
  - Integration test in `chat_composer.rs` asserting `Ctrl+M` opens the shell prompt view and allows input.

## Notes

- This feature aids debugging and inspection without leaving the agent workflow.
- Ensure that security policies (e.g. sandbox restrictions) still apply to these commands.
