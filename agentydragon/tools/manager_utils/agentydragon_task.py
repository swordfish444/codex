"""
CLI for managing agentydragon tasks: status, set-status, set-deps, dispose, launch.
"""
import subprocess
import re
import sys
from datetime import datetime

import click
from tasklib import load_task, repo_root, save_task, task_dir, TaskMeta, worktree_dir
import shutil

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
    # Load all task metadata, reporting load errors with file path
    all_meta: dict[str, TaskMeta] = {}
    path_map: dict[str, Path] = {}
    wt_root = worktree_dir()
    for md in sorted(task_dir().rglob('[0-9][0-9]-*.md')):
        # skip task template, plan files, and any worktree copies
        if md.name in ('task-template.md',) or md.name.endswith('-plan.md') or md.is_relative_to(wt_root):
            continue
        try:
            meta, _ = load_task(md)
        except Exception as e:
            print(f"Error loading {md}: {e}")
            continue
        all_meta[meta.id] = meta
        path_map[meta.id] = md

    # If a worktree exists, reload the task from that workspace (including .done paths)
    repo = repo_root()
    for tid, md in list(path_map.items()):
        wt_root_dir = wt_root / md.stem
        # derive relative path of the task file under the repo
        try:
            rel = md.relative_to(repo)
        except Exception:
            continue
        wt_task = wt_root_dir / rel
        if wt_task.exists():
            try:
                wt_meta, _ = load_task(wt_task)
                all_meta[tid] = wt_meta
                path_map[tid] = wt_task
            except Exception as e:
                print(f"Error loading {wt_task}: {e}")

    # Build dependency graph, excluding already merged tasks
    merged_ids = {tid for tid, m in all_meta.items() if m.status == 'Merged'}
    deps_map: dict[str, list[str]] = {}
    for tid, meta in all_meta.items():
        deps_map[tid] = [d for d in re.findall(r"\d+", meta.dependencies)
                         if d in all_meta and d not in merged_ids]

    # Topologically sort tasks by dependencies, fall back on filename order on error
    try:
        sorted_ids: list[str] = []
        temp: set[str] = set()
        perm: set[str] = set()
        def visit(n: str) -> None:
            if n in perm:
                return
            if n in temp:
                raise RuntimeError(f"Circular dependency detected at task {n}")
            temp.add(n)
            for m in deps_map.get(n, []):
                visit(m)
            temp.remove(n)
            perm.add(n)
            sorted_ids.append(n)
        for n in all_meta:
            visit(n)
    except Exception as e:
        print(f"Warning: cannot topo-sort tasks ({e}); falling back to filename order")
        sorted_ids = [m.id for m in sorted(all_meta.values(), key=lambda m: path_map[m.id].name)]

    # Identify tasks that are merged with no branch and no worktree (bottom summary)
    bottom_merged_ids: set[str] = set()
    for tid in sorted_ids:
        meta = all_meta[tid]
        if meta.status != 'Merged':
            continue
        branches = subprocess.run(
            ['git', 'for-each-ref', '--format=%(refname:short)',
             f'refs/heads/agentydragon-{tid}-*'],
            capture_output=True, text=True, cwd=repo_root()
        ).stdout.strip().splitlines()
        wt_dir = task_dir() / '.worktrees' / path_map[tid].stem
        if not branches and not wt_dir.exists():
            bottom_merged_ids.add(tid)

    rows: list[tuple] = []
    merged_tasks: list[tuple[str, str]] = []
    root = repo_root()

    for tid in sorted_ids:
        meta = all_meta[tid]
        md = path_map[tid]
        slug = md.stem
        # branch detection
        branches = subprocess.run(
            ['git', 'for-each-ref', '--format=%(refname:short)',
             f'refs/heads/agentydragon-{tid}-*'],
            capture_output=True, text=True, cwd=root
        ).stdout.strip().splitlines()
        branch_exists = 'Y' if branches and branches[0].strip() else 'N'
        merged_flag = 'N'
        if branch_exists == 'Y':
            b = branches[0].lstrip('*+ ').strip()
            if subprocess.run(['git', 'merge-base', '--is-ancestor', b, 'agentydragon'], cwd=root).returncode == 0:
                merged_flag = 'Y'
        # worktree detection
        wt_dir = worktree_dir() / slug
        wt_info = 'none'
        if wt_dir.exists():
            st = subprocess.run(['git', 'status', '--porcelain'], cwd=wt_dir,
                                capture_output=True, text=True).stdout.strip()
            wt_info = 'clean' if not st else 'dirty'

        # skip fully merged tasks (no branch, no worktree)
        if meta.status == 'Merged' and branch_exists == 'N' and wt_info == 'none':
            merged_tasks.append((tid, meta.title))
            continue

        # filter out dependencies on bottom-summary merged tasks
        deps = [d for d in deps_map.get(tid, []) if d not in bottom_merged_ids]
        deps_str = ','.join(deps)

        # determine branch_info text
        if branch_exists == 'N':
            branch_info = 'no branch'
        elif merged_flag == 'Y':
            branch_info = 'merged'
        else:
            a_cnt, b_cnt = subprocess.check_output(
                ['git', 'rev-list', '--left-right', '--count',
                 f'{branches[0]}...agentydragon'], cwd=root
            ).decode().split()
            # compact diffstat: e.g. "56 files changed, 1265 insertions(+), 342 deletions(-)" -> "56f,1265i,342d"
            raw = subprocess.check_output(
                ['git', 'diff', '--shortstat', f'{branches[0]}...agentydragon'], cwd=root
            ).decode().strip()
            stat = (
                raw.replace(' files changed', 'f')
                   .replace(' file changed', 'f')
                   .replace(' insertions(+)', 'i')
                   .replace(' deletions(-)', 'd')
                   .replace(', ', ',')
            )
            base = subprocess.check_output(
                ['git', 'merge-base', 'agentydragon', branches[0]], cwd=root
            ).decode().strip()
            mtree = subprocess.check_output(
                ['git', 'merge-tree', base, 'agentydragon', branches[0]], cwd=root
            ).decode(errors='ignore')
            conflict = 'conflict' if '<<<<<<<' in mtree else 'ok'
            if a_cnt == '0' and b_cnt == '0':
                branch_info = f'up-to-date (+{stat or 0})'
            else:
                branch_info = f'{b_cnt} behind / {a_cnt} ahead (+{stat or 0}) {conflict}'

        # Normalize and colorize status/worktree
        raw_status = str(meta.status)
        # Strip enum class prefix if present (e.g. TaskStatus.Done)
        cls_name = meta.status.__class__.__name__
        prefix = cls_name + "."
        if raw_status.startswith(prefix):
            raw_status = raw_status[len(prefix):]
        # Color-map statuses
        status_colors = {
            'Not started': '\033[90m',   # dim gray
            'In progress': '\033[33m',   # yellow
            'Needs input': '\033[31m',    # red
            'Needs manual review': '\033[31m',
            'Done': '\033[32m',           # green
            'Cancelled': '\033[31m',      # red
            'Merged': '\033[34m',         # blue
        }
        color = status_colors.get(raw_status, '')
        if color:
            stat_disp = f"{color}{raw_status}\033[0m"
        else:
            stat_disp = raw_status
        wt_disp = wt_info
        if wt_info == 'dirty':
            wt_disp = f"\033[31m{wt_info}\033[0m"

        rows.append((
            tid, meta.title, stat_disp,
            deps_str, meta.last_updated.strftime('%Y-%m-%d %H:%M'),
            branch_info, wt_disp
        ))

    headers = ['ID', 'Title', 'Status', 'Dependencies', 'Updated',
               'Branch Status', 'Worktree Status']
    if tabulate:
        print(tabulate(rows, headers=headers, tablefmt='github'))
    else:
        fmt = '{:>2}  {:<30}  {:<12}  {:<20}  {:<16}  {:<40}  {:<10}'
        print(fmt.format(*headers))
        for r in rows:
            print(fmt.format(*r))

    # summary of fully merged tasks (no branch, no worktree)
    if merged_tasks:
        items = ' '.join(f"{tid} ({title})" for tid, title in merged_tasks)
        print(f"\n\033[32mMerged:\033[0m {items}")

    # summary of tasks Ready to merge (Done with branch commits)
    ready_tasks: list[tuple[str, str]] = []
    for tid in sorted_ids:
        meta = all_meta[tid]
        if meta.status != 'Done':
            continue
        # detect branch existence and ahead commits
        branches = subprocess.run(
            ['git', 'for-each-ref', '--format=%(refname:short)', f'refs/heads/agentydragon-{tid}-*'],
            capture_output=True, text=True, cwd=repo_root()
        ).stdout.strip().splitlines()
        if not branches or not branches[0].strip():
            continue
        bname = branches[0].lstrip('*+ ').strip()
        # count commits ahead of integration branch
        a_cnt, _b_cnt = subprocess.check_output(
            ['git', 'rev-list', '--left-right', '--count', f'{bname}...agentydragon'], cwd=repo_root()
        ).decode().split()
        if int(a_cnt) > 0:
            ready_tasks.append((tid, meta.title))
    if ready_tasks:
        items = ' '.join(f"{tid} ({title})" for tid, title in ready_tasks)
        print(f"\n\033[33mReady to merge:\033[0m {items}")

    # identify unblocked tasks (no remaining dependencies)
    unblocked = [tid for tid in sorted_ids if tid not in merged_ids and not deps_map.get(tid)]
    if unblocked:
        print(f"\n\033[1mUnblocked:\033[0m {' '.join(unblocked)}")
        print(f"\033[1mLaunch unblocked in tmux:\033[0m python agentydragon/tools/create_task_worktree.py --agent --tmux {' '.join(unblocked)}")

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
    root = repo_root()
    wt_base = worktree_dir()
    for tid in task_id:
        # Remove any matching worktree directories
        g = f'{tid}-*'
        matching_wts = wt_base.glob(g)
        for wt_dir in matching_wts:
            click.echo(f"Disposing worktree {wt_dir}")
            # unregister worktree; then delete the directory if still present
            rel = wt_dir.relative_to(root)
            subprocess.run(['git', 'worktree', 'remove', str(rel), '--force'], cwd=root)
            if wt_dir.exists():
                shutil.rmtree(wt_dir)
        else:
            print(f"No worktrees matching {g} in {wt_base}")
        # prune any stale worktree entries
        subprocess.run(['git', 'worktree', 'prune'], cwd=root)
        # Delete any matching branches
        # delete any matching local branches cleanly via for-each-ref
        ref_pattern = f'refs/heads/agentydragon-{tid}-*'
        branches = subprocess.run(
            ['git', 'for-each-ref', '--format=%(refname:short)', ref_pattern],
            capture_output=True, text=True, cwd=root
        ).stdout.splitlines()
        branches = [br for br in branches if br]
        if branches:
            click.echo(f"Disposing branches: {branches}")
            subprocess.run(['git', 'branch', '-D', *branches], cwd=root)
        else:
            click.echo(f"No branches matching {ref_pattern}")
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
