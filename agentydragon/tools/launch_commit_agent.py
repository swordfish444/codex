#!/usr/bin/env python3
"""
launch_commit_agent.py: Run the non-interactive Commit agent for completed tasks.
"""
import os
import subprocess
import sys
from pathlib import Path

import click

from common import repo_root, tasks_dir, worktrees_dir, resolve_slug


@click.command()
@click.argument('task_input', required=True)
def main(task_input):
    """Resolve TASK_INPUT to slug, run the Commit agent, and commit changes."""
    slug = resolve_slug(task_input)
    wt = worktrees_dir() / slug
    if not wt.exists():
        click.echo(f"Error: worktree for '{slug}' not found; run create_task_worktree.py first", err=True)
        sys.exit(1)

    prompt_file = repo_root() / 'agentydragon' / 'prompts' / 'commit.md'
    task_file = tasks_dir() / f'{slug}.md'
    for f in (prompt_file, task_file):
        if not f.exists():
            click.echo(f"Error: file not found: {f}", err=True)
            sys.exit(1)

    msg_file = Path(subprocess.check_output(['mktemp']).decode().strip())
    try:
        os.chdir(wt)
        # Abort early if no pending changes in this worktree
        status_out = subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()
        if not status_out:
            click.echo(f"No changes detected in worktree for '{slug}'; nothing to commit.", err=True)
            sys.exit(0)
        cmd = ['codex', '--full-auto', 'exec', '--output-last-message', str(msg_file)]
        click.echo(f"Running: {' '.join(cmd)}")
        prompt_content = prompt_file.read_text(encoding='utf-8')
        task_content = task_file.read_text(encoding='utf-8')
        subprocess.check_call(cmd + [prompt_content + '\n\n' + task_content])
        # Stage all changes, including new files (not just modifications)
        subprocess.check_call(['git', 'add', '-A'])
        subprocess.check_call(['git', 'commit', '-F', str(msg_file)])
    finally:
        msg_file.unlink()


if __name__ == '__main__':
    main()
