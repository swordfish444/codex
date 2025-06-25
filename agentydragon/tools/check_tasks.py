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

from manager_utils.tasklib import task_dir, worktree_dir, load_task


def skip_path(p: Path) -> bool:
    """Return True for paths we should ignore in validations."""
    wt = worktree_dir()
    done = task_dir() / ".done"
    if p.is_relative_to(wt) or p.is_relative_to(done):
        return True
    if p.name in ("task-template.md",) or p.name.endswith("-plan.md"):
        return True
    return False


def check_file_types():
    failures: list[Path] = []
    for p in task_dir().iterdir():
        if skip_path(p) or p.is_dir():
            continue
        if p.suffix.lower() != ".md":
            failures.append(p)
    return failures


def check_frontmatter():
    failures: list[tuple[Path, str]] = []
    wt = worktree_dir()
    for md in task_dir().rglob("[0-9][0-9]-*.md"):
        if skip_path(md):
            continue
        try:
            load_task(md)
        except Exception as e:
            failures.append((md, str(e)))
    return failures


def check_cycles():
    merged = set()
    deps_map: dict[str, list[str]] = {}
    wt = worktree_dir()
    for md in task_dir().rglob("[0-9][0-9]-*.md"):
        if skip_path(md):
            continue
        meta, _ = load_task(md)
        if meta.status == "Merged":
            merged.add(meta.id)
        else:
            deps = [d for d in re.findall(r"\d+", meta.dependencies)]
            deps_map[meta.id] = [d for d in deps if d not in merged]

    failures: list[list[str]] = []
    visited: set[str] = set()
    stack: list[str] = []

    def visit(n: str):
        if n in stack:
            cycle = stack[stack.index(n) :] + [n]
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
        print("Non-md files under tasks/:", file=sys.stderr)
        for f in ft_fail:
            print(f"  {f}", file=sys.stderr)
        err = True

    # Frontmatter check
    fm_fail = check_frontmatter()
    if fm_fail:
        print("\nFrontmatter errors:", file=sys.stderr)
        for md, msg in fm_fail:
            print(f"  {md}: {msg}", file=sys.stderr)
        err = True

    # Dependency cycles
    cyc_fail = check_cycles()
    if cyc_fail:
        print("\nCircular dependency errors:", file=sys.stderr)
        for cycle in cyc_fail:
            print("  " + " -> ".join(cycle), file=sys.stderr)
        err = True

    if err:
        sys.exit(1)
    print("All task checks passed.")


if __name__ == "__main__":
    main()
