#!/usr/bin/env python3
"""
organize_done_tasks.py: Move merged task files under tasks/.done/ subdirectory.

This script should be run once to migrate all tasks with status "Merged"
to the .done folder.
"""
import subprocess
from pathlib import Path

from tasklib import task_dir, load_task

def main():
    root = task_dir()
    done_dir = root / '.done'
    done_dir.mkdir(exist_ok=True)
    for md in sorted(root.glob('[0-9][0-9]-*.md')):
        if md.name == 'task-template.md' or md.name.endswith('-plan.md'):
            continue
        meta, _ = load_task(md)
        if meta.status == 'Merged':
            target = done_dir / md.name
            print(f'Moving {md.name} -> .done/')
            subprocess.run(['git', 'mv', str(md), str(target)], check=True)
    print('Migration complete.')

if __name__ == '__main__':
    main()
