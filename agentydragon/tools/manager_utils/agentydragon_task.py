"""
CLI for managing agentydragon tasks: status, set-status, set-deps, dispose, launch.
"""
import subprocess
import sys
from datetime import datetime

import click
from tasklib import load_task, repo_root, save_task, task_dir

try:
    from tabulate import tabulate
except ImportError:
    tabulate = None


@click.group()
def cli():
    """Manage agentydragon tasks."""
    pass

@cli.command()
def status():
    """Show a table of task id, title, status, dependencies, last_updated.

    If tabulate is installed, render as GitHub-flavored Markdown table;
    otherwise fallback to fixed-width formatting.
    """
    rows = []
    root = repo_root()
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
        # list matching task branch names cleanly
        branches = subprocess.run(
            ['git', 'for-each-ref', '--format=%(refname:short)', f'refs/heads/agentydragon-{meta.id}-*'],
            capture_output=True, text=True, cwd=root
        ).stdout.strip().splitlines()
        branch_exists = 'Y' if branches and branches[0].strip() else 'N'
        merged = 'N'
        if branch_exists == 'Y':
            bname = branches[0].lstrip('*+ ').strip()
            merged = 'Y' if subprocess.run(
                ['git', 'merge-base', '--is-ancestor', bname, 'agentydragon'],
                cwd=root
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
        # derive branch & merge status
        if branches:
            bname = branches[0]
            # merged into agentydragon?
            is_merged = subprocess.run(
                ['git', 'merge-base', '--is-ancestor', bname, 'agentydragon'],
                cwd=root
            ).returncode == 0
            if is_merged:
                branch_info = 'merged'
            else:
                # ahead/behind
                a_cnt, b_cnt = subprocess.check_output(
                    ['git', 'rev-list', '--left-right', '--count', f'{bname}...agentydragon'],
                    cwd=root
                ).decode().split()
                # diffstat
                stat = subprocess.check_output(
                ['git', 'diff', '--shortstat', f'{bname}...agentydragon'], cwd=root
                ).decode().strip()
                diffstat = stat.replace(' file changed', '')
                # merge conflict scan
                base = subprocess.check_output(
                ['git', 'merge-base', 'agentydragon', bname], cwd=root
                ).decode().strip()
                mtree = subprocess.check_output(
                ['git', 'merge-tree', base, 'agentydragon', bname], cwd=root
                ).decode(errors='ignore')
                conflict = 'conflict' if '<<<<<<<' in mtree else 'ok'
                if a_cnt == '0' and b_cnt == '0':
                    branch_info = f'up-to-date (+{diffstat or 0})'
                else:
                    branch_info = f'{b_cnt} behind / {a_cnt} ahead (+{diffstat or 0}) {conflict}'
        else:
            branch_info = 'no branch'
        # worktree status
        wt_dir = task_dir() / '.worktrees' / slug
        if wt_dir.exists():
            wt_clean = 'clean' if not subprocess.run(
                ['git', 'status', '--porcelain'], cwd=wt_dir,
                capture_output=True, text=True
            ).stdout.strip() else 'dirty'
            wt_info = wt_clean
        else:
            wt_info = 'none'
        rows.append((
            meta.id, meta.title, meta.status,
            meta.dependencies.replace('\n', ' '),
            meta.last_updated.strftime('%Y-%m-%d %H:%M'),
            branch_info, wt_info
        ))
    headers = ['ID', 'Title', 'Status', 'Dependencies', 'Updated',
               'Branch Status', 'Worktree Status']
    if tabulate:
        # render as Markdown table if tabulate is available
        print(tabulate(rows, headers=headers, tablefmt='github'))
    else:
        # fallback to plain fixed-width formatting
        fmt = (
            '{:>2}  {:<30}  {:<12}  {:<20}  {:<16}  {:<40}  {:<10}'
        )
        print(fmt.format(*headers))
        for r in rows:
            print(fmt.format(*r))

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
