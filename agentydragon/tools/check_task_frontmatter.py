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
import pathlib

try:
    import yaml
except ImportError:
    print("Missing dependency: PyYAML is required to run this script.", file=sys.stderr)
    sys.exit(1)

REQUIRED_KEYS = ["id", "title", "status", "summary", "goal"]
ALLOWED_STATUSES = ["Not started", "Started", "Needs manual review", "Done", "Cancelled"]

def parse_frontmatter(text):
    # Expect frontmatter delimited by '---' on its own line
    lines = text.splitlines()
    if len(lines) < 3 or lines[0].strip() != '---':
        return None
    try:
        end = lines[1:].index('---') + 1
    except ValueError:
        return None
    front = '\n'.join(lines[1:end])
    try:
        data = yaml.safe_load(front)
    except yaml.YAMLError as e:
        print(f"YAML parse error: {e}")
        return None
    return data

def main():
    root = pathlib.Path(__file__).resolve().parent.parent
    tasks_dir = root / 'agentydragon' / 'tasks'
    failures = 0

    for md in tasks_dir.glob('[0-9][0-9]-*.md'):
        if md.name == 'task-template.md' or md.name.endswith('-plan.md'):
            continue
        text = md.read_text(encoding='utf-8')
        data = parse_frontmatter(text)
        if not data:
            print(f"{md}: missing or malformed YAML frontmatter")
            failures += 1
            continue
        for key in REQUIRED_KEYS:
            if key not in data:
                print(f"{md}: missing required frontmatter key '{key}'")
                failures += 1
        status = data.get('status')
        if status not in ALLOWED_STATUSES:
            print(f"{md}: invalid status '{status}'; must be one of {ALLOWED_STATUSES}")
            failures += 1

    if failures:
        print(f"\nFound {failures} frontmatter errors.", file=sys.stderr)
        sys.exit(1)
    print("All task frontmatter OK.")
    sys.exit(0)

if __name__ == '__main__':
    main()