#!/usr/bin/env python3
"""
create_task_worktree.py: Create or reuse a git worktree for a specific task and optionally launch a Developer Codex agent.
"""
import os
import subprocess
import sys
from pathlib import Path

import click

from common import repo_root, tasks_dir, worktrees_dir, resolve_slug


def run(cmd, cwd=None):
    click.echo(f"Running: {' '.join(cmd)}")
    subprocess.check_call(cmd, cwd=cwd)


def resolve_slug(input_id: str) -> str:
    if input_id.isdigit() and len(input_id) == 2:
        matches = list(tasks_dir().glob(f"{input_id}-*.md"))
        if len(matches) == 1:
            return matches[0].stem
        click.echo(f"Error: expected one task file for ID {input_id}, found {len(matches)}", err=True)
        sys.exit(1)
    return input_id


@click.command()
@click.option('-a', '--agent', is_flag=True,
              help='Launch Developer Codex agent after setting up worktree.')
@click.option('-t', '--tmux', 'tmux_mode', is_flag=True,
              help='Open each task in its own tmux pane; implies --agent.')
@click.option('-i', '--interactive', is_flag=True,
              help='Run agent in interactive mode (no exec); implies --agent.')
@click.option('-s', '--shell', 'shell_mode', is_flag=True,
              help='Launch an interactive Codex shell (skip auto-commit); implies --agent.')
@click.argument('task_inputs', nargs=-1, required=True)
def main(agent, tmux_mode, interactive, shell_mode, task_inputs):
    """Create/reuse a task worktree and optionally launch a Dev agent or tmux session."""
    if interactive or shell_mode:
        agent = True

    if tmux_mode:
        agent = True
        session = 'agentydragon_' + '_'.join(task_inputs)
        for idx, inp in enumerate(task_inputs):
            slug = resolve_slug(inp)
            cmd = [sys.executable, '-u', __file__]
            if agent:
                cmd.append('--agent')
            cmd.append(slug)
            if idx == 0:
                run(['tmux', 'new-session', '-d', '-s', session] + cmd)
            else:
                run(['tmux', 'new-window', '-t', session] + cmd)
        run(['tmux', 'attach', '-t', session])
        return

    # Single task
    slug = resolve_slug(task_inputs[0])
    branch = f"agentydragon-{slug}"
    wt_root = worktrees_dir()
    wt_path = wt_root / slug

    # Ensure branch exists
    if subprocess.call(['git', 'show-ref', '--verify', '--quiet', f'refs/heads/{branch}']) != 0:
        run(['git', 'branch', '--track', branch, 'agentydragon'])

    wt_root.mkdir(parents=True, exist_ok=True)
    if not wt_path.exists():
        # Create worktree without checkout, then hydrate via reflink/rsync for COW performance
        run(['git', 'worktree', 'add', '--no-checkout', str(wt_path), branch])
        src = str(repo_root())
        dst = str(wt_path)
        try:
            run(['cp', '-cRp', f'{src}/.', f'{dst}/', '--exclude=.git', '--exclude=.gitlink'])
        except subprocess.CalledProcessError:
            run(['rsync', '-a', '--delete', f'{src}/', f'{dst}/', '--exclude=.git*'])
        if shutil.which('pre-commit'):
            run(['pre-commit', 'install'], cwd=dst)
        else:
            click.echo('Warning: pre-commit not found; skipping hook install', err=True)
    else:
        click.echo(f'Worktree already exists at {wt_path}')

    if not agent:
        return

    # Pre-commit checks
    if shutil.which('pre-commit'):
        run(['pre-commit', 'run', '--all-files'], cwd=str(wt_path))
    else:
        click.echo('Warning: pre-commit not installed; skipping checks', err=True)

    click.echo(f'Launching Developer Codex agent for task {slug} in sandboxed worktree')
    os.chdir(wt_path)
    cmd = ['codex', '--full-auto']
    if not interactive:
        cmd.append('exec')
    prompt = (repo_root() / 'agentydragon' / 'prompts' / 'developer.md').read_text()
    taskfile = (tasks_dir() / f'{slug}.md').read_text()
    run(cmd + [prompt + '\n\n' + taskfile])


if __name__ == '__main__':
    import shutil
    main()
