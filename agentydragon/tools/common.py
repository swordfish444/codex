#!/usr/bin/env python3
"""
common.py: Shared utilities for agentydragon tooling scripts.
"""
import subprocess
from pathlib import Path


def repo_root() -> Path:
    """Return the Git repository root directory."""
    out = subprocess.check_output(['git', 'rev-parse', '--show-toplevel'])
    return Path(out.decode().strip())


def tasks_dir() -> Path:
    """Path to the agentydragon/tasks directory."""
    return repo_root() / 'agentydragon' / 'tasks'


def worktrees_dir() -> Path:
    """Path to the agentydragon/tasks/.worktrees directory."""
    return tasks_dir() / '.worktrees'


def resolve_slug(input_id: str) -> str:
    """Resolve a two-digit task ID into its full slug, or return slug unchanged."""
    if input_id.isdigit() and len(input_id) == 2:
        matches = list(tasks_dir().glob(f"{input_id}-*.md"))
        if len(matches) == 1:
            return matches[0].stem
        raise ValueError(f"Expected one task file for ID {input_id}, found {len(matches)}")
    return input_id
