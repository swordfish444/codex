+++
id = "04"
title = "Auto-Mount Entire Repo and Auto-CD to Subfolder"
status = "Not started"
dependencies = "01" # Rationale: depends on Task 01 for mount-add/remove foundational commands
last_updated = "2025-06-25T01:40:09.800000"
+++

# Task 04: Auto-Mount Entire Repo and Auto-CD to Subfolder

> *This task is specific to codex-rs.*

## Subtasks

Subtasks to implement in order all in one P:

### 04.1 – Config → `ConfigToml` + `Config`
- Add `auto_mount_repo: bool` and `mount_prefix: String` to `ConfigToml` (with proper `#[serde(default)]` and defaults).
- Wire these fields through to the `Config` struct.

### 04.2 – Git root detection + relative‐path
- Implement a helper in `codex_core::util` to locate the Git repository root given a starting `cwd`.
- Compute the sub‐directory path relative to the repo root.

### 04.3 – Bind‑mount logic
- In the sandbox startup path (`apply_sandbox_policy_to_current_thread` or a new wrapper before it), if `auto_mount_repo` is set:
  - Bind‑mount `repo_root` → `mount_prefix` (e.g. `/workspace`).
  - Create target directory if missing.

### 04.4 – Automate `cwd` → new mount
- After mounting, update the process‐wide `cwd` to `mount_prefix/relative_path` so all subsequent file ops occur under the mount.

### 04.5 – Config docs & tests
- Update `config.md` to document `auto_mount_repo` and `mount_prefix` under the top‐level config.
- Add unit tests for the Git‐root helper and default values.

### 04.6 – E2E manual verification
- Manually verify launching with `auto_mount_repo = true` in a nested subfolder:
  - TTY prompt shows sandboxed cwd under `/workspace/<subdir>`.
  - Commands executed by Codex see the mount.

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
