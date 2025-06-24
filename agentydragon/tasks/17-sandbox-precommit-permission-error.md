---
id: 17
title: Sandbox Pre-commit Permission Error
status: Not started
summary: Pre-commit hooks fail in sandbox due to inability to lock user gitconfig.
goal: |
  Investigate and resolve pre-commit setup failures in sandbox environments caused by permission errors on ~/.gitconfig so that pre-commit checks can run reliably within agent worktrees.
---

> *This task addresses scaffolding/setup for Agent worktrees.*

## Acceptance Criteria

- Pre-commit hooks detect sandbox environment and skip or override gitconfig locking.
- Documentation in scaffold guides is updated to note pre-commit limitations and workarounds.
- Verification steps demonstrate pre-commit hooks succeeding in sandbox without modifying user gitconfig.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- The sandbox prevents locking ~/.gitconfig, leading to PermissionError.
- Consider configuring pre-commit to use a repo-local config or skip locking by passing `--config` or setting `PRE_COMMIT_HOME`.