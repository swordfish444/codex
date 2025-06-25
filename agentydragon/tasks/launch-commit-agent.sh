#!/usr/bin/env bash
#
# launch-commit-agent.sh
#
# Launch the non-interactive Codex Commit agent for a given task worktree,
# using the prompt in prompts/commit.md and the task's Markdown file.
#
# Usage: launch-commit-agent.sh <task-slug|NN>

set -euo pipefail

# Accept either a two-digit task ID or full slug (NN-slug)
if [ "$#" -ne 1 ]; then
  echo "Usage: $0 <task-slug|NN>" >&2
  exit 1
fi
input="$1"

# Locate repository and directories
repo_root=$(git rev-parse --show-toplevel)
tasks_dir="$repo_root/agentydragon/tasks"

# Resolve numeric ID to full slug if needed
if [[ "$input" =~ ^[0-9]{2}$ ]]; then
  matches=("$tasks_dir/${input}-"*.md)
  if [ "${#matches[@]}" -eq 1 ]; then
    task_slug="$(basename "${matches[0]}" .md)"
    echo "Resolved task ID '$input' to slug '$task_slug'"
  else
    echo "Error: expected exactly one task file matching '${input}-*.md', found ${#matches[@]}" >&2
    exit 1
  fi
else
  task_slug="$input"
fi

# Paths for worktree and prompt/task files
worktrees_dir="$tasks_dir/.worktrees"
worktree_path="$worktrees_dir/$task_slug"
prompt_file="$repo_root/agentydragon/prompts/commit.md"
task_file="$tasks_dir/$task_slug.md"

# Verify worktree exists
if [ ! -d "$worktree_path" ]; then
  echo "Error: worktree for '$task_slug' not found; run create-task-worktree.sh first" >&2
  exit 1
fi

# Verify prompt and task files exist
if [ ! -f "$prompt_file" ]; then
  echo "Error: commit prompt not found at $prompt_file" >&2
  exit 1
fi
if [ ! -f "$task_file" ]; then
  echo "Error: task file not found at $task_file" >&2
  exit 1
fi

# Change to the task worktree and invoke Codex in non-interactive mode
cd "$worktree_path"
# Invoke the Commit agent and pipe its output into git commit
cmd=(codex --full-auto exec)
echo "Running: ${cmd[*]}"
message=$("${cmd[@]}" "$(<"$prompt_file")"$'\n\n'"$(<"$task_file")")
# Stage all changes and commit with generated message
git add -u
git commit -m "$message"
