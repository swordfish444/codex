+++
id = "09"
title = "File- and Directory-Level Approvals"
status = "Not started"
dependencies = "11" # Rationale: depends on Task 11 for custom approval predicate infrastructure
last_updated = "2025-06-25T01:40:09.507043"
+++

# Task 09: File- and Directory-Level Approvals

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal

Enable fine-grained approval controls so users can whitelist edits scoped to specific files or directories at runtime, with optional time limits.

## Acceptance Criteria

- In the approval dialog, offer “Allow this file always” and “Allow this directory always” options alongside proceed/deny.
- Prompt for a time limit when granting a file/dir approval, with default presets (e.g. 5 min, 1 hr, 4 hr, 24 hr).
- Introduce runtime commands to inspect and manage granular approvals:
  - `/approvals list` to view active approvals and remaining time
  - `/approvals add [file|dir] <path> [--duration <preset>]` to grant approval
  - `/approvals remove <id>` to revoke an approval
- Persist granular approvals in session metadata, keyed by working directory. On session resume in a different directory, warn the user and discard all file/dir approvals.
- Automatically expire and remove approvals when their time limits elapse.
- Reflect file/dir-approval state in the CLI shell prompt or title for quick visibility.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- Store approvals with {id, scope: file|dir, path, expires_at} in session JSON.
- Use a background timer or check-before-command to prune expired entries.
- Reuse existing command-parsing infrastructure to implement `/approvals` subcommands.
- Consider UI/UX for selecting presets in TUI dialogs.
