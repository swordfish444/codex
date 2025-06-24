# Task 06: External Editor Integration for Prompt Entry

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal
Allow users to spawn an external editor (e.g. Neovim) to compose or edit the chat prompt. The prompt box should update with the editor's contents when closed.

## Acceptance Criteria
- A slash command `/edit-prompt` (or `Ctrl+E`) launches the user's preferred editor on a temporary file pre-populated with the current draft.
- Upon editor exit, the draft is re-read into the composer widget.
- Configurable via `editor = "${VISUAL:-${EDITOR:-nvim}}"` setting in `config.toml`.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes
- Leverage the existing file-opener machinery or spawn a subprocess directly.
  Modify `tui/src/bottom_pane/chat_composer.rs` and command handling in `tui/src/app.rs`.