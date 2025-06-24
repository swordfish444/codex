# Task 15: Agent Worktree Sandbox Configuration

## Status

**General Status**: Not started  
**Summary**: Enhance the task scaffolding script to launch a Codex agent in a sandboxed worktree where only the task directory (and system temp dir) is writable, Git commands run without prompts, and all file I/O under the worktree is auto-approved.

## Goal

Use `create-task-worktree.sh --agent` to wrap the agent invocation in a sandbox with these properties:
- The task worktree path and the system temporary directory (`$TMPDIR` or `/tmp`) are mounted read-write.
- All other paths on the host are treated as read-only.
- Git operations in the worktree (e.g. `git add`, `git commit`) succeed without additional confirmation.
- Any file read or write under the worktree root is automatically approved.

## Acceptance Criteria

The `create-task-worktree.sh --agent` invocation:
- launches the agent via `codex debug landlock` (or equivalent), passing flags to mount only the worktree and tempdir as writable.
- sets up Landlock permissions so that all other host paths are read-only.
- auto-approves any file system operation under the worktree directory.
- auto-approves Git commands in the worktree without prompting.
- still permits using system temp dir for ephemeral files.
- contains tests or manual verifications demonstrating blocked writes outside and allowed writes inside.

## Implementation

**How it was implemented**  
*(Not implemented yet)*
- Modify `create-task-worktree.sh --agent`:
  - Detect `$TMPDIR` (or default `/tmp`) and include it in the writable mount list.
  - Invoke the agent via `codex debug landlock` (or chosen sandbox command) with `--writable-root` for the worktree and tempdir.
  - Add approval predicates to auto-allow any file I/O under the worktree path and Git commands there.
- Update the script’s help text (`-h|--help`) to document the sandbox behavior and tempdir whitelist.
- Add tests or example runs verifying sandbox restrictions and approvals.

**How it works**  
*(Not implemented yet)*  
When `--agent` is used, the script switches to the task worktree, then starts the sandbox so that only the worktree and the system tempdir are writable. Inside that sandbox, Git and other file operations under the worktree proceed without prompts, while writes elsewhere on the host are blocked.

## Notes

- This feature depends on the underlying Landlock/Seatbelt sandbox APIs.  
- Leverage the existing sandbox invocation (`codex debug landlock`) and approval predicates to auto-approve worktree and tmpdir I/O.