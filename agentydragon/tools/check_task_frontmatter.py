#!/usr/bin/env python3
"""
check_task_frontmatter.py: Validate structured YAML frontmatter in task Markdown files.

This script ensures each task file under agentydragon/tasks/ has a YAML frontmatter block
with the required keys: id, title, status, summary, and goal.
It also enforces that `status` is one of the allowed enum values.

Usage:
    python3 agentydragon/tools/check_task_frontmatter.py

Returns exit code 0 if all files pass, 1 otherwise.
"""

import sys

from manager_utils import tasklib

try:
    import yaml
except ImportError:
    print("Missing dependency: PyYAML is required to run this script.", file=sys.stderr)
    sys.exit(1)

REQUIRED_KEYS = ["id", "title", "status", "summary", "goal"]
ALLOWED_STATUSES = ["Not started", "Started", "Needs manual review", "Done", "Cancelled", "Merged", "Reopened"]

def main():
    failures = 0

    # skip template/plan files and any worktree copies
    wt_root = tasklib.worktree_dir()
    for md in tasklib.task_dir().rglob('[0-9][0-9]-*.md'):
        if md.name == 'task-template.md' or md.name.endswith('-plan.md') or md.is_relative_to(wt_root):
            continue
        try:
            task, _body = tasklib.load_task(md)
        except ValueError as e:
            print(f"{md}: {e}", file=sys.stderr)
            failures += 1

    if failures:
        print(f"\nFound {failures} frontmatter errors.", file=sys.stderr)
        sys.exit(1)
    print("All task frontmatter OK.")
    sys.exit(0)

if __name__ == '__main__':
    main()
