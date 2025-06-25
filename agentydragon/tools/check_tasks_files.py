#!/usr/bin/env python3
"""
check_tasks_files.py: Pre-commit hook to ensure only Markdown files in agentydragon/tasks/ (excluding .worktrees and .done).
"""
import sys
from pathlib import Path

def main():
    bad = []
    for f in sys.argv[1:]:
        p = Path(f)
        # skip worktree copies and done archives
        if p.is_relative_to(Path('agentydragon/tasks/.worktrees')) or p.is_relative_to(Path('agentydragon/tasks/.done')):
            continue
        # allow only .md files
        if p.suffix.lower() != '.md':
            bad.append(f)
    if bad:
        print('Error: only Markdown (.md) files are allowed under agentydragon/tasks/:', file=sys.stderr)
        for f in bad:
            print(f'  {f}', file=sys.stderr)
        sys.exit(1)
    return 0

if __name__ == '__main__':
    sys.exit(main())
