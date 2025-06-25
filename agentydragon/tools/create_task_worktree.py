#!/usr/bin/env python3
"""
create_task_worktree.py: Create or reuse a git worktree for a specific task and optionally launch a Developer Codex agent.
"""
import os
import subprocess
import sys
import re
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
              help='Open each task in its own tmux pane; implies --agent. '
                   'Attaches to an existing session if already running.')
@click.option('-i', '--interactive', is_flag=True,
              help='Run agent in interactive mode (no exec); implies --agent.')
@click.option('-s', '--shell', 'shell_mode', is_flag=True,
              help='Launch an interactive Codex shell (skip exec and auto-commit); implies --agent and --interactive.')
@click.option('--skip-presubmit', is_flag=True,
              help='Skip the initial presubmit pre-commit checks when creating a new worktree.')
@click.argument('task_inputs', nargs=-1, required=True)
def main(agent, tmux_mode, interactive, shell_mode, skip_presubmit, task_inputs):
    """Create/reuse a task worktree and optionally launch a Dev agent or tmux session."""
    # shell mode implies interactive (skip exec within the worktree)
    if shell_mode:
        interactive = True
    if interactive or shell_mode:
        agent = True

    if tmux_mode:
        agent = True
        session = 'agentydragon_' + '_'.join(task_inputs)
        # If a tmux session already exists, skip setup and attach
        if subprocess.call(['tmux', 'has-session', '-t', session]) == 0:
            click.echo(f"Session {session} already exists; attaching")
            run(['tmux', 'attach', '-t', session])
            return
        # Create a new session and windows for each task
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
    new_wt = False
    if not wt_path.exists():
        # --- COW hydration logic via rsync ---
        # Instead of checking out files normally, register the worktree empty and then
        # perform a filesystem-level hydration via rsync (with reflink if supported) for
        # near-instant setup while excluding VCS metadata and other worktrees.
        run(['git', 'worktree', 'add', '--no-checkout', str(wt_path), branch])
        src = str(repo_root())
        dst = str(wt_path)
        # Hydrate the worktree filesystem via rsync, excluding .git and any .worktrees to avoid recursion
        rsync_cmd = [
            'rsync', '-a', '--delete', f'{src}/', f'{dst}/',
            '--exclude=.git*', '--exclude=.worktrees/'
        ]
        if sys.platform != 'darwin':
            rsync_cmd.insert(3, '--reflink=auto')
        run(rsync_cmd)
        # Install pre-commit hooks in the new worktree
        if shutil.which('pre-commit'):
            run(['pre-commit', 'install'], cwd=dst)
        else:
            click.echo('Warning: pre-commit not found; skipping hook install', err=True)
        new_wt = True
    else:
        click.echo(f'Worktree already exists at {wt_path}')

    if not agent:
        return

    # Initial presubmit: only on new worktree & branch, unless skipped or in shell mode
    if new_wt and not skip_presubmit and not shell_mode:
        if shutil.which('pre-commit'):
            try:
                run(['pre-commit', 'run', '--all-files'], cwd=str(wt_path))
            except subprocess.CalledProcessError:
                click.echo(
                    'Pre-commit checks failed. Please fix the issues in the worktree or ' +
                    're-run with --skip-presubmit to bypass these checks.', err=True)
                sys.exit(1)
        else:
            click.echo('Warning: pre-commit not installed; skipping presubmit checks', err=True)

    click.echo(f'Launching Developer Codex agent for task {slug} in sandboxed worktree')

    click.echo(f'Launching Developer Codex agent for task {slug} in sandboxed worktree')
    os.chdir(wt_path)
    cmd = ['codex', '--full-auto']
    if not interactive:
        cmd.append('exec')
    prompt = (repo_root() / 'agentydragon' / 'prompts' / 'developer.md').read_text()
    taskfile = (tasks_dir() / f'{slug}.md').read_text()
    run(cmd + [prompt + '\n\n' + taskfile])
    # After Developer agent exits, if task status is Done, invoke Commit agent to stage and commit changes
    task_path = tasks_dir() / f"{slug}.md"
    content = task_path.read_text(encoding='utf-8')
    m = re.search(r'^status\s*=\s*"([^"]+)"', content, re.MULTILINE)
    status = m.group(1) if m else None
    if status and status.lower() == 'done':
        click.echo(f"Task {slug} marked Done; running Commit agent helper")
        commit_script = repo_root() / 'agentydragon' / 'tools' / 'launch_commit_agent.py'
        # Launch commit agent from the main repo root, not inside the task worktree
        run([sys.executable, str(commit_script), slug], cwd=str(repo_root()))
    else:
        click.echo(f"Task {slug} status is '{status or 'unknown'}'; skipping Commit agent helper")


if __name__ == '__main__':
    import shutil
    main()
