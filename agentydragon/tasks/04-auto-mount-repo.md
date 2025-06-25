+++
id = "04"
title = "Auto-Mount Entire Repo and Auto-CD to Subfolder"
status = "Not started"
dependencies = "01" # Rationale: depends on Task 01 for mount-add/remove foundational commands
last_updated = "2025-06-25T01:40:09.800000"
+++

# Task 04: Auto-Mount Entire Repo and Auto-CD to Subfolder

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Not started; missing Implementation details (How it was implemented and How it works).

## Goal
Allow users to enable a flag so that each session:

1. Detects the Git repository root of the current working directory.
2. Bind-mounts the entire repository into `/workspace` in the session.
3. Changes directory to `/workspace/<relative-path-from-root>` to mirror the user’s original subfolder.

## Acceptance Criteria
- New `auto_mount_repo = true` and optional `mount_prefix = "/workspace"` in `config.toml`.
- Before any worktree or mount processing, detect the Git root, bind-mount it to `mount_prefix`, and set `cwd` to `mount_prefix + relative_path`.
- Existing worktree/session-worktree logic should operate relative to this new `cwd`.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes
- This offloads the entire monorepo into the session, leaving the user’s original clone untouched.
