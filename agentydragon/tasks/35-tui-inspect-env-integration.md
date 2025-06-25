+++
id = "35"
title = "TUI Integration for Inspect-Env Command"
status = "Not started"
dependencies = "10" # Rationale: depends on Task 10 for container state inspection
last_updated = "2025-06-25T04:45:29Z"
+++

> *This task is specific to codex-rs.*

## Status

**General Status**: In progress  
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

**High-level approach**
- Extend `SlashCommand` enum with `InspectEnv` and provide user-visible description.
- Add `InlineInspectEnv` variant to `AppEvent` enum to represent inline slash-command invocation.
- Update dispatch logic in `App::run` to spawn a background thread on `InlineInspectEnv` that runs `codex inspect-env`, reads its stdout line-by-line, and sends each line as `AppEvent::LatestLog`, then triggers a redraw.
- Wire up `/inspect-env` to dispatch `InlineInspectEnv` in the slash-command handling.
- Add unit tests in the TUI crate to verify `built_in_slash_commands()` includes `inspect-env` mapping and description.

**How it works**  
When the user enters `/inspect-env`, the TUI parser recognizes the command and emits `AppEvent::InlineInspectEnv`.  The main event loop handles this event by spawning a thread that invokes the external `codex inspect-env` command, captures its output line-by-line, and forwards each line into the TUI log pane via `AppEvent::LatestLog`. A redraw is scheduled once the inspection completes.

## Notes

- Reuse formatting code from `cli/src/inspect_env.rs` for consistency.
