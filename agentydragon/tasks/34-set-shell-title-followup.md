+++
id = "34"
title = "Complete Set Shell Title to Reflect Session Status"
status = "Not started"
dependencies = "08" # Rationale: depends on Task 08 for initial shell title change
last_updated = "2025-06-25T04:45:29Z"
+++

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Follow-up to Task 08; implementation missing for core title persistence and ANSI updates.

## Goal

Implement the missing pieces from Task 08 to fully support dynamic and persistent shell title updates:
1. Define `SessionUpdatedTitleEvent` and add a `title` field in `SessionConfiguredEvent` (core protocol).
2. Introduce `Op::SetTitle(String)` variant and handle it in the core agent loop, persisting the title and emitting the update event.
3. Update TUI and exec clients to listen for title events and emit ANSI escape sequences (`\x1b]0;<title>\x07`) for live terminal title changes.
4. Restore the persisted title on session resume via `SessionConfiguredEvent`.

## Acceptance Criteria

- New `SessionUpdatedTitleEvent` type in `codex_core::protocol` and `title` field in `SessionConfiguredEvent`.
- `Op::SetTitle(String)` variant in the protocol and core event handling persisted in session metadata.
- Clients broadcast ANSI title-setting sequences on title events and lifecycle state changes.
- Unit tests for protocol serialization and client reaction to title updates.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Use ANSI escape code `\x1b]0;<title>\x07` for setting terminal title.
