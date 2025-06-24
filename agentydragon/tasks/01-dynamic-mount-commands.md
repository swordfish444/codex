# Task 01: Dynamic Mount-Add and Mount-Remove Commands

> *This task is specific to codex-rs.*

## Status

**General Status**: Completed  
**Summary**: Implemented inline DSL and interactive dialogs for `/mount-add` and `/mount-remove`, with dynamic sandbox policy updates.

## Goal
Implement the `/mount-add` and `/mount-remove` slash commands in the TUI, supporting two modes:

1. **Inline DSL**: e.g. `/mount-add host=/path/to/host container=/path/in/agent mode=rw`
2. **Interactive dialog**: if the user just types `/mount-add` or `/mount-remove` without args, pop up a prompt to fill in `host`, `container`, and optional `mode` fields.

These commands should:
- Create or remove symlinks (or real directories) under the current working directory.
- Update the in-memory `SandboxPolicy` to grant or revoke read/write permission for the host path.
- Emit confirmation or error messages into the TUI log pane.

## Acceptance Criteria
- Users can type `/mount-add host=... container=... mode=...` and the mount is created immediately.
- Users can type `/mount-add` alone to open a small TUI form prompting for the three fields.
- Symmetrically for `/mount-remove` by container path.
- The `sandbox_policy` is updated so subsequent shell commands can read/write the newly mounted folder.

## Implementation

**How it was implemented**  
- Added two new slash commands (`mount-add`, `mount-remove`) to the TUI’s `slash-command` popup.
- Inline DSL parsing: commands typed as `/mount-add host=... container=... mode=...` or `/mount-remove container=...` are detected and handled immediately by parsing key/value args, performing the mount/unmount, and updating the `Config.sandbox_policy` in memory.
- Interactive dialogs: selecting `/mount-add` or `/mount-remove` without args opens a bottom‑pane form (`MountAddView` or `MountRemoveView`) that prompts sequentially for the required fields and then triggers the same mount logic.
- Mount logic implemented in `do_mount_add`/`do_mount_remove`:
  - Creates/removes a symlink under `cwd` pointing to the host path (`std::os::unix::fs::symlink` on Unix, platform equivalents on Windows).
  - Uses new `SandboxPolicy` methods (`allow_disk_write_folder`/`revoke_disk_write_folder`) to grant or revoke `DiskWriteFolder` permissions for the host path.
  - Emits success or error messages via `tracing::info!`/`tracing::error!`, which appear in the TUI log pane.

**How it works**  
1. **Inline DSL**  
   - User types:  
     ```
     /mount-add host=/path/to/host container=path/in/cwd mode=ro
     ```
   - The first-stage popup intercepts the mount-add command with args, dispatches `InlineMountAdd`, and the app parses the args and runs the mount logic immediately.
2. **Interactive dialog**  
   - User types `/mount-add` (or selects it via the popup) without args.
   - A small form appears that prompts for `host`, `container`, then `mode`.
   - Upon completion, the same mount logic runs.
3. **Unmount**  
   - `/mount-remove container=...` (inline) or `/mount-remove` (interactive) remove the symlink and revoke write permissions.
4. **Policy update**  
   - `allow_disk_write_folder` appends a `DiskWriteFolder` permission for new mounts.
   - `revoke_disk_write_folder` removes the corresponding permission on unmount.

## Notes
- This builds on the static `[[sandbox.mounts]]` support introduced earlier.