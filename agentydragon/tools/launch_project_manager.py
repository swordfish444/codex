#!/usr/bin/env python3
"""
launch_project_manager.py: Launch the Codex Project Manager agent prompt.
"""
import subprocess
import sys

import click

from common import repo_root


@click.command()
def main():
    """Read manager.md prompt and invoke Codex Project Manager agent."""
    prompt_file = repo_root() / 'agentydragon' / 'prompts' / 'manager.md'
    if not prompt_file.exists():
        click.echo(f"Error: manager prompt not found at {prompt_file}", err=True)
        sys.exit(1)

    prompt = prompt_file.read_text(encoding='utf-8')
    cmd = ['codex', prompt]
    click.echo(f"Running: {' '.join(cmd[:1])} <prompt>")
    subprocess.check_call(cmd)


if __name__ == '__main__':
    main()
