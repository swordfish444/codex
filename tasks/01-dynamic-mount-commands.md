# Task 01: Dynamic Mount-Add and Mount-Remove Commands

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

## Notes
- This builds on the static `[[sandbox.mounts]]` support introduced earlier.