+++
id = "03"
title = "Live Config Reload and Prompt on Changes"
status = "Not started"
dependencies = "02,07,09,11,14,29"
last_updated = "2025-06-25T01:40:09.504758"
+++

# Task 03: Live Config Reload and Prompt on Changes

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal
Detect changes to the user `config.toml` file while a session is running and prompt the user to apply or ignore the updated settings.

## Acceptance Criteria
- A background file watcher watches `$CODEX_HOME/config.toml` (or active user config path).
- On any write event, compute a unified diff between the in-memory config and the on-disk file.
- Pause the agent, display the diff in the TUI bottom pane, and offer two actions: `Apply new config now` or `Continue with old config`.
- If the user applies, re-parse the config, merge overrides, and resume using the new settings. Otherwise, discard changes and resume.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes
- Leverage a crate such as `notify` for FS events and `similar` or `diff` for unified diff generation.
