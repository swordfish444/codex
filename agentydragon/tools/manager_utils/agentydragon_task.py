"""
CLI for managing agentydragon tasks: status, set-status, set-deps, dispose, launch.
"""
import sys
import subprocess
from datetime import datetime
from pathlib import Path
import click
from tasklib import load_task, save_task, TaskMeta

TASK_DIR = Path(__file__).parent.parent / 'tasks'

@click.group()
def cli():
    """Manage agentydragon tasks."""
    pass

@cli.command()
def status():
    """Show a table of task id, title, status, dependencies, last_updated"""
    rows = []
    for md in sorted(TASK_DIR.glob('[0-9][0-9]-*.md')):
        meta, _ = load_task(md)
        rows.append((meta.id, meta.title, meta.status,
                     meta.dependencies.replace('\n', ' '),
                     meta.last_updated.strftime('%Y-%m-%d %H:%M')))
    # simple table
    fmt = '{:>2}  {:<50}  {:<12}  {:<30}  {:<16}'
    print(fmt.format('ID','Title','Status','Dependencies','Updated'))
    for r in rows:
        print(fmt.format(*r))

@cli.command()
@click.argument('task_id')
@click.argument('status')
def set_status(task_id, status):
    """Set status of TASK_ID to STATUS"""
    md = TASK_DIR / f"{task_id}-*.md"
    files = list(TASK_DIR.glob(f'{task_id}-*.md'))
    if not files:
        click.echo(f'Task {task_id} not found', err=True)
        sys.exit(1)
    path = files[0]
    meta, body = load_task(path)
    meta.status = status
    meta.last_updated = datetime.utcnow()
    save_task(path, meta, body)

@cli.command()
@click.argument('task_id')
@click.argument('deps', nargs=-1)
def set_deps(task_id, deps):
    """Set dependencies of TASK_ID"""
    files = list(TASK_DIR.glob(f'{task_id}-*.md'))
    if not files:
        click.echo(f'Task {task_id} not found', err=True)
        sys.exit(1)
    path = files[0]
    meta, body = load_task(path)
    now = datetime.utcnow().isoformat()
    meta.dependencies = f'as of {now}: ' + ', '.join(deps)
    meta.last_updated = datetime.utcnow()
    save_task(path, meta, body)

@cli.command()
@click.argument('task_id', nargs=-1)
def dispose(task_id):
    """Dispose worktree and delete branch for TASK_ID(s)"""
    for tid in task_id:
        branch = f'agentydragon-{tid}-*'
        # remove worktree
        subprocess.run(['git', 'worktree', 'remove', f'tasks/.worktrees/{tid}-*', '--force'])
        # delete branch
        subprocess.run(['git', 'branch', '-D', f'agentydragon-{tid}-*'])
        click.echo(f'Disposed task {tid}')

@cli.command()
@click.argument('task_id', nargs=-1)
def launch(task_id):
    """Copy tmux launch one-liner for TASK_ID(s) to clipboard"""
    cmd = ['create-task-worktree.sh', '--agent', '--tmux'] + list(task_id)
    line = ' '.join(cmd)
    # system clipboard
    try:
        subprocess.run(['pbcopy'], input=line.encode(), check=True)
        click.echo('Copied to clipboard:')
    except FileNotFoundError:
        click.echo(line)
        return
    click.echo(line)

if __name__ == '__main__':
    cli()
