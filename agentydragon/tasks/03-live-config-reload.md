+++
id = "03"
title = "Live Config Reload and Prompt on Changes"
status = "Done"
dependencies = "02,07,09,11,14,29"
last_updated = "2025-06-25T01:40:09.504758"
+++

# Task 03: Live Config Reload and Prompt on Changes

> *This task is specific to codex-rs.*

## Status

**General Status**: Done  
**Summary**: Live config watcher, diff prompt, and reload integration implemented.

## Goal
Detect changes to the user `config.toml` file while a session is running and prompt the user to apply or ignore the updated settings.

## Acceptance Criteria
- A background file watcher watches `$CODEX_HOME/config.toml` (or active user config path).
- On any write event, compute a unified diff between the in-memory config and the on-disk file.
- Pause the agent, display the diff in the TUI bottom pane, and offer two actions: `Apply new config now` or `Continue with old config`.
- If the user applies, re-parse the config, merge overrides, and resume using the new settings. Otherwise, discard changes and resume.

## Implementation

**How it was implemented**  
- Added `codex_tui::config_reload::generate_diff` to compute unified diffs via the `similar` crate (with a unit test).  
- Spawned a `notify`-based filesystem watcher thread in `tui::run_main` that debounces write events on `$CODEX_HOME/config.toml`, generates diffs against the last-read contents, and posts `AppEvent::ConfigReloadRequest(diff)`.
- Introduced `AppEvent` variants (`ConfigReloadRequest`, `ConfigReloadApply`, `ConfigReloadIgnore`) and wired them in `App::run` to display a new `BottomPaneView` overlay.
- Created `BottomPaneView` implementation `ConfigReloadView` to render the diff and handle `<Enter>`/`<Esc>` for apply or ignore.
- On apply, reloaded `Config` via `Config::load_with_cli_overrides`, updated both `App.config` and `ChatWidget` (rebuilding its bottom pane with updated settings).

**How it works**  
- The watcher thread detects on-disk changes and pushes a diff request into the UI event loop.
- Upon `ConfigReloadRequest`, the TUI bottom pane overlays the diff view and blocks normal input.
- `<Enter>` applies the new config (re-parses and updates runtime state); `<Esc>` dismisses the overlay and continues with the old settings.

## Notes
- Leverage a crate such as `notify` for FS events and `similar` or `diff` for unified diff generation.
