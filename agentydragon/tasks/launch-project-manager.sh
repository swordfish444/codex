#!/usr/bin/env bash
#
# launch-project-manager.sh
#
# Launch the Codex Project Manager agent using the prompt in prompts/manager.md.

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
prompt_file="$repo_root/agentydragon/prompts/manager.md"

if [ ! -f "$prompt_file" ]; then
  echo "Error: manager prompt file not found at $prompt_file" >&2
  exit 1
fi

codex "$(<"$prompt_file")"