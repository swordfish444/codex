# Task 04 Plan: Auto‑Mount Entire Repo & Auto‑CD

> *This task is specific to codex-rs.*

## Status

**General Status**: Not started  
**Summary**: Planning phase; missing Implementation details (How it was implemented and How it works).

We’ll break Task 04 into discrete subtasks so we can implement, review, and test each part in isolation:

## Subtasks

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

## Next steps
Please review the plan above. If it looks good, I’ll implement the subtasks in order.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*