+++
id = "15"
title = "Agent Worktree Sandbox Configuration"
status = "Merged"
dependencies = "02,07,09,11,14,29"
last_updated = "2025-06-25T07:26:13.570520"
+++

# Task 15: Agent Worktree Sandbox Configuration

## Status

**General Status**: Done  
**Summary**: Enhanced the task scaffolding script to launch a Codex agent in a sandboxed worktree with writable worktree and TMPDIR, auto-approved file I/O and Git operations, and network disabled.

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
- Extended `create-task-worktree.sh` `--agent` mode to launch the Codex agent under a Landlock+seccomp sandbox by invoking `codex debug landlock --full-auto`, which grants write access only to the worktree (`cwd`) and the platform temp folder (`TMPDIR`), and disables network.  
- Updated the `-a|--agent` help text to reflect the new sandbox behavior and tempdir whitelist.  
- Added `agentydragon/tasks/15-sandbox-test.sh`, a test script demonstrating allowed writes inside the worktree and TMPDIR and blocked writes to directories outside those paths.  

**How it works**  
When invoked with `--agent`, `create-task-worktree.sh` changes into the task worktree and launches:

```bash
codex debug landlock --full-auto codex "$(< \"$repo_root/agentydragon/prompts/developer.md\")"
```

The `--full-auto` flag configures Landlock to allow disk writes under the current directory and the system temp directory, disable network access, and automatically approve commands on success. As a result, any file I/O and Git operations in the worktree proceed without approval prompts, while writes outside the worktree and TMPDIR are blocked by the sandbox.

## Notes

- This feature depends on the underlying Landlock/Seatbelt sandbox APIs.  
- Leverage the existing sandbox invocation (`codex debug landlock`) and approval predicates to auto-approve worktree and tmpdir I/O.
