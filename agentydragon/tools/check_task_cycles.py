#!/usr/bin/env python3
"""
check_task_cycles.py: Pre-commit hook to detect circular dependencies among non-merged tasks.
"""
import re
import sys
from pathlib import Path

# allow importing from manager_utils by adding tool directory to PYTHONPATH
sys.path.insert(0, str(Path(__file__).parent.parent))
from manager_utils.tasklib import task_dir, load_task

def main():
    # Load all tasks and separate merged vs non-merged
    merged = set()
    deps_map = {}
    id_to_path = {}
    # skip template/plan files and any worktree copies
    wt_root = task_dir() / '.worktrees'
    for md in task_dir().rglob('[0-9][0-9]-*.md'):
        if md.name == 'task-template.md' or md.name.endswith('-plan.md') or md.is_relative_to(wt_root):
            continue
        meta, _ = load_task(md)
        id_to_path[meta.id] = md
        if meta.status == 'Merged':
            merged.add(meta.id)
        else:
            # extract numeric dependencies
            deps = [d for d in re.findall(r"\d+", meta.dependencies)]
            deps_map[meta.id] = deps

    # filter out dependencies on merged tasks
    for tid in deps_map:
        deps_map[tid] = [d for d in deps_map[tid] if d not in merged]

    # detect cycles via DFS
    visited = set()
    stack = []

    def visit(n):
        if n in stack:
            cycle = stack[stack.index(n):] + [n]
            cycle_str = ' -> '.join(cycle)
            print(f"Circular dependency detected: {cycle_str}", file=sys.stderr)
            print("Paths involved in cycle:", file=sys.stderr)
            for tid in cycle:
                print(f"  {id_to_path.get(tid, tid)}", file=sys.stderr)
            sys.exit(1)
        if n in visited:
            return
        stack.append(n)
        for m in deps_map.get(n, []):
            visit(m)
        stack.pop()
        visited.add(n)

    for node in deps_map:
        if node not in visited:
            visit(node)

if __name__ == '__main__':
    main()
