+++
id = "06"
title = "External Editor Integration for Prompt Entry"
status = "Done"
dependencies = "02,07,09,11,14,29"
last_updated = "2025-06-25T01:40:09.505778"
+++

# Task 06: External Editor Integration for Prompt Entry

> *This task is specific to codex-rs.*

## Status

**General Status**: Done  
**Summary**: External editor integration for prompt entry implemented.

## Goal
Allow users to spawn an external editor (e.g. Neovim) to compose or edit the chat prompt. The prompt box should update with the editor's contents when closed.

## Acceptance Criteria
- A slash command `/edit-prompt` (or `Ctrl+E`) launches the user's preferred editor on a temporary file pre-populated with the current draft.
- Upon editor exit, the draft is re-read into the composer widget.
- Configurable via `editor = "${VISUAL:-${EDITOR:-nvim}}"` setting in `config.toml`.

## Implementation

**How it was implemented**  
- Added `editor` option to `[tui]` section in `config.toml`, defaulting to `${VISUAL:-${EDITOR:-nvim}}`.  
- Exposed the `tui.editor` setting in the `codex-core` config model (`config_types.rs`) and wired it through to the TUI.  
- Added a new slash-command variant `EditPrompt` in `tui/src/slash_command.rs` to trigger external-editor mode.  
- Implemented `ChatComposer::open_external_editor()` in `tui/src/bottom_pane/chat_composer.rs`:  
  - Creates a temporary file pre-populated with the current draft prompt.  
  - Launches the configured editor (from `VISUAL`/`EDITOR` with `nvim` fallback) in a blocking subprocess.  
  - Reads the edited contents back into the `TextArea` on editor exit.  
- Wired both `Ctrl+E` and the `/edit-prompt` slash command to invoke `open_external_editor()`.  
- Updated `config.md` to document the new `editor` setting under `[tui]`.

**How it works**  
- Pressing `Ctrl+E`, or typing `/edit-prompt` and hitting Enter, spawns the user's preferred editor on a temporary file containing the current draft.  
- When the editor process exits, the plugin reads back the file and updates the chat composer with the edited text.  
- The default editor is determined by `VISUAL`, then `EDITOR`, falling back to `nvim` if neither is set.
