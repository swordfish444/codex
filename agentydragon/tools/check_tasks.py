#!/usr/bin/env python3
"""
check_tasks.py: Run all task-directory validation checks in one go.
  - Ensure task Markdown frontmatter parses and validates (id, title, status, etc.).
  - Detect circular dependencies among non-merged tasks.
  - Enforce only .md files under agentydragon/tasks/ (excluding .worktrees/ and .done/).
"""
import re
import sys
from pathlib import Path

import toml
import yaml
from manager_utils.tasklib import TaskMeta, task_dir, worktree_dir, load_task

def check_file_types():
    failures = []
    wt_root = worktree_dir()
    done_root = task_dir() / '.done'
    for f in task_dir().iterdir():
        if not f.is_file():
            continue
        if f.is_relative_to(wt_root) or f.is_relative_to(done_root):
            continue
        if f.suffix.lower() != '.md':
            failures.append(f)
    return failures

def check_frontmatter():
    failures = []
    wt_root = worktree_dir()
    for md in task_dir().rglob('[0-9][0-9]-*.md'):
        if md.name in ('task-template.md',) or md.name.endswith('-plan.md') or md.is_relative_to(wt_root):
            continue
        try:
            load_task(md)
        except Exception as e:
            failures.append((md, str(e)))
    return failures

def check_cycles():
    merged = set()
    deps_map = {}
    wt_root = worktree_dir()
    for md in task_dir().rglob('[0-9][0-9]-*.md'):
        if md.name in ('task-template.md',) or md.name.endswith('-plan.md') or md.is_relative_to(wt_root):
            continue
        meta, _ = load_task(md)
        if meta.status == 'Merged':
            merged.add(meta.id)
        else:
            deps = re.findall(r"\d+", meta.dependencies)
            deps_map[meta.id] = [d for d in deps if d not in merged]

    failures = []
    visited = set()
    stack = []

    def visit(n):
        if n in stack:
            cycle = stack[stack.index(n):] + [n]
            failures.append(cycle)
            return
        if n in visited:
            return
        stack.append(n)
        for m in deps_map.get(n, []):
            visit(m)
        stack.pop()
        visited.add(n)

    for node in deps_map:
        visit(node)
    return failures

def main():
    err = False

    # File type check
    ft_fail = check_file_types()
    if ft_fail:
        print('Non-md files under tasks/:', file=sys.stderr)
        for f in ft_fail:
            print(f'  {f}', file=sys.stderr)
        err = True

    # Frontmatter check
    fm_fail = check_frontmatter()
    if fm_fail:
        print('\nFrontmatter errors:', file=sys.stderr)
        for md, msg in fm_fail:
            print(f'  {md}: {msg}', file=sys.stderr)
        err = True

    # Dependency cycles
    cyc_fail = check_cycles()
    if cyc_fail:
        print('\nCircular dependency errors:', file=sys.stderr)
        for cycle in cyc_fail:
            print('  ' + ' -> '.join(cycle), file=sys.stderr)
        err = True

    if err:
        sys.exit(1)
    print('All task checks passed.')

if __name__ == '__main__':
    main()
