+++
id = "35"
title = "TUI Integration for Inspect-Env Command"
status = "Not started"
dependencies = "10"
last_updated = "2025-06-25T04:45:29Z"
+++

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Follow-up to Task 10; add slash-command and TUI bindings for `inspect-env`.

## Goal

Add an `/inspect-env` slash-command in the TUI that invokes the existing `codex inspect-env` logic to display sandbox state inline.

## Acceptance Criteria

- Extend `SlashCommand` enum to include `InspectEnv`.
- Dispatch `AppEvent::InlineInspectEnv` when `/inspect-env` is entered.
- Handle `InlineInspectEnv` in `app.rs` to run `inspect-env` logic and stream its output to the TUI log pane.
- Render mounts, permissions, and network status in a formatted table or tree view in the bottom pane.
- Unit/integration tests simulating slash-command invocation and verifying rendered output.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Reuse formatting code from `cli/src/inspect_env.rs` for consistency.
