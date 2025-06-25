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
    for md in task_dir().rglob('[0-9][0-9]-*.md'):
        if md.name == 'task-template.md' or md.name.endswith('-plan.md'):
            continue
        meta, _ = load_task(md)
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
            print(f"Circular dependency detected: {' -> '.join(cycle)}")
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
