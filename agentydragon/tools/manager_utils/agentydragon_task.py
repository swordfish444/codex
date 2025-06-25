"""
CLI for managing agentydragon tasks: status, set-status, set-deps, dispose, launch.
"""
import subprocess
import sys
from datetime import datetime
from pathlib import Path

import click
from tasklib import load_task, save_task, TaskMeta


@click.group()
def cli():
    """Manage agentydragon tasks."""
    pass

def repo_root():
    return Path(subprocess.check_output(['git', 'rev-parse', '--show-toplevel']).decode().strip())

def task_dir():
    return repo_root() / "agentydragon/tasks"

@cli.command()
def status():
    """Show a table of task id, title, status, dependencies, last_updated"""
    rows = []
    for md in sorted(task_dir().glob('*.md')):
        if md.name == 'task-template.md' or md.name.endswith('-plan.md'):
            continue
        try:
            meta, _ = load_task(md)
        except ValueError as e:
            print(e)
            continue
        slug = md.stem
        # branch detection
        branches = subprocess.run(
            ['git', 'branch', '--list', f'agentydragon-{meta.id}-*'],
            capture_output=True, text=True, cwd=repo_root()
        ).stdout.strip().splitlines()
        branch_exists = 'Y' if branches and branches[0].strip() else 'N'
        merged = 'N'
        if branch_exists == 'Y':
            bname = branches[0].lstrip('* ').strip()
            merged = 'Y' if subprocess.run(
                ['git', 'merge-base', '--is-ancestor', bname, 'agentydragon'],
                cwd=repo_root()
            ).returncode == 0 else 'N'
        # worktree detection
        wt_dir = task_dir() / '.worktrees' / slug
        wt_exists = wt_dir.exists()
        wt_clean = 'NA'
        if wt_exists:
            status_out = subprocess.run(
                ['git', 'status', '--porcelain'], cwd=wt_dir,
                capture_output=True, text=True
            ).stdout.strip()
            wt_clean = 'Clean' if not status_out else 'Dirty'
        rows.append((
            meta.id, meta.title, meta.status,
            meta.dependencies.replace('\n', ' '),
            meta.last_updated.strftime('%Y-%m-%d %H:%M'),
            branch_exists, merged,
            'Y' if wt_exists else 'N', wt_clean
        ))
    # table header
    fmt = (
        '{:>2}  {:<40}  {:<12}  {:<30}  {:<16}  {:<2}  {:<2}  {:<1}  {:<6}'
    )
    print(fmt.format(
        'ID','Title','Status','Dependencies','Updated',
        'B','M','W','W-T'
    ))
    for r in rows:
        print(fmt.format(*r))

@cli.command()
def applyfrontmatter():
    """Add TOML frontmatter to task files missing it."""
    for md in sorted(task_dir().glob('*.md')):
        if md.name == 'task-template.md' or md.name.endswith('-plan.md'):
            continue
        try:
            load_task(md)
            continue
        except ValueError:
            pass
        text = md.read_text(encoding='utf-8')
        # parse id from filename prefix
        task_id = md.stem.split('-', 1)[0]
        # parse title
        title = ''
        for line in text.splitlines():
            if line.startswith('# Task '):
                parts = line.split(':', 1)
                title = parts[1].strip() if len(parts) == 2 else line.lstrip('# ').strip()
                break
        if not title:
            click.echo(f'Could not parse title from {md}', err=True)
            continue
        # parse status
        status = ''
        in_status = False
        for line in text.splitlines():
            if in_status and line.strip().startswith('**General Status**:'):
                status = line.split(':', 1)[1].strip()
                break
            if line.strip() == '## Status':
                in_status = True
        if not status:
            status = 'Not started'
        meta = TaskMeta(id=task_id, title=title, status=status)
        save_task(md, meta, text)
        click.echo(f'Applied TOML frontmatter to {md}')

@cli.command()
@click.argument('task_id')
@click.argument('status')
def set_status(task_id, status):
    """Set status of TASK_ID to STATUS"""
    md = task_dir() / f"{task_id}-*.md"
    files = list(task_dir().glob(f'{task_id}-*.md'))
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
    files = list(task_dir().glob(f'{task_id}-*.md'))
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
