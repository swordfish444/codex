+++
id = "15"
title = "Embedded Neovim Prompt Editor"
status = "Not started"
dependencies = ""
last_updated = "2025-06-25T01:40:09.513224"
+++

# Task 15: Embedded Neovim Prompt Editor

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal

Replace the basic line‑editing prompt composer with an embedded Neovim window so users can enjoy full-featured, multi-line editing of their chat prompt directly inside the TUI.

## Acceptance Criteria

- Introduce a TUI-integrated Neovim editor pane activated via `/edit-prompt` or `Ctrl+E` when `embedded_prompt_editor = true` in `[tui]` config.
- Pre-populate the Neovim buffer with the current draft prompt; upon exit, reload the buffer contents back into the composer.
- Support standard Neovim keybindings and commands (e.g. insert mode, visual mode, plugins) within the embedded pane.
- Cleanly restore the previous TUI layout after closing the editor, with prompt focus returned to the composer.
- Provide configuration toggle (`embedded_prompt_editor`) and fall back to external-editor prompt behavior when disabled.

## Implementation

**How it was implemented**  
- Add a new module `tui/src/editor/neovim.rs` that wraps a headless Neovim RPC instance and renders its UI into a dedicated TUI layer.
- Extend `tui/src/bottom_pane/chat_composer.rs` to detect `embedded_prompt_editor` and invoke the embedded editor instead of spawning an external process.
- Wire a config flag `embedded_prompt_editor: bool` through `ConfigToml` → `Config` under the `tui` section, defaulting to `false`.
- Handle Neovim communication via `nvim-rs` crate, multiplexing input/output over the TUI event loop.

**How it works**  
- When the user triggers the editor, pause the main TUI rendering and allocate a full-screen or split view for Neovim.
- Start Neovim in embedded RPC mode, passing the current prompt text into a new buffer.
- Drive Neovim’s UI updates via RPC and render its screen cells into the TUI terminal using termion or similar backend.
- Detect the Neovim exit event (e.g. user `:q` or `ZZ`), fetch the buffer contents, and close the embedded view.
- Restore the original TUI state and update the composer widget with the edited prompt.

## Notes

- This relies on a working `nvim` binary in PATH or specified via `nvim_binary` config.
- Investigate performance impact of embedding a full editor in the TUI; ensure fallback to external-editor remains smooth.
- Consider edge cases (resizing, plugin‑heavy Neovim configs) and document prerequisites in the README.