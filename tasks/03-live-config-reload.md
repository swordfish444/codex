# Task 03: Live Config Reload and Prompt on Changes

## Goal
Detect changes to the user `config.toml` file while a session is running and prompt the user to apply or ignore the updated settings.

## Acceptance Criteria
- A background file watcher watches `$CODEX_HOME/config.toml` (or active user config path).
- On any write event, compute a unified diff between the in-memory config and the on-disk file.
- Pause the agent, display the diff in the TUI bottom pane, and offer two actions: `Apply new config now` or `Continue with old config`.
- If the user applies, re-parse the config, merge overrides, and resume using the new settings. Otherwise, discard changes and resume.

## Notes
- Leverage a crate such as `notify` for FS events and `similar` or `diff` for unified diff generation.