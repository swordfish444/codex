# Task 06: External Editor Integration for Prompt Entry

> *This task is specific to codex-rs.*

## Status

**General Status**: In progress  
**Summary**: Implementation underway; configuration, slash-command, keybinding, and editor-integration code to be added.

## Goal
Allow users to spawn an external editor (e.g. Neovim) to compose or edit the chat prompt. The prompt box should update with the editor's contents when closed.

## Acceptance Criteria
- A slash command `/edit-prompt` (or `Ctrl+E`) launches the user's preferred editor on a temporary file pre-populated with the current draft.
- Upon editor exit, the draft is re-read into the composer widget.
- Configurable via `editor = "${VISUAL:-${EDITOR:-nvim}}"` setting in `config.toml`.

## Implementation

**How it was implemented**  
1. Added `prompt_editor` setting to the TUI config (defaults to `$VISUAL`, `$EDITOR`, or `nvim`).
2. Introduced `SlashCommand::EditPrompt` (`/edit-prompt`) and bound Ctrl+E in the composer to dispatch it.
3. Implemented `ChatComposer::open_external_editor()` to spawn the external editor on a temporary file pre-filled with the current draft and reload its contents on exit.
4. Exposed `open_external_editor()` via `BottomPane::open_external_editor()` and `ChatWidget::open_external_editor()`, and wired up `EditPrompt` in `App`.
5. Updated documentation (`config.md`) and added a test for the Ctrl+E mapping in the composer.

**How it works**  
When the user types `/edit-prompt` or presses Ctrl+E in the chat input, the composer writes its buffer to a temp file, launches the configured editor on that file (with raw mode suspended), and upon successful exit reads the file back into the composer widget, resetting the cursor to the start. Errors are logged via `tracing::error!`.

## Notes
- Leverage the existing file-opener machinery or spawn a subprocess directly.
  Modify `tui/src/bottom_pane/chat_composer.rs` and command handling in `tui/src/app.rs`.
