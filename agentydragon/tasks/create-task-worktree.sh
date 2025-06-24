#!/usr/bin/env bash
#
# create-task-worktree.sh
#
# Create or reuse a git worktree for a specific task branch under agentydragon/tasks/.worktrees.
# Usage: create-task-worktree.sh <task-id>-<task-slug>

set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 <task-id>-<task-slug>"
  exit 1
fi

task_slug="$1"
branch="agentydragon/$task_slug"

# Determine repository root
repo_root=$(git rev-parse --show-toplevel)

tasks_dir="$repo_root/agentydragon/tasks"
worktrees_dir="$tasks_dir/.worktrees"
worktree_path="$worktrees_dir/$task_slug"

mkdir -p "$worktrees_dir"

# Create branch if it does not exist
if ! git show-ref --verify --quiet "refs/heads/$branch"; then
  echo "Creating branch $branch from master..."
  git branch --track "$branch" master
fi

# Create worktree if it does not exist
if [ ! -d "$worktree_path" ]; then
  echo "Creating worktree for $branch at $worktree_path"
  git worktree add "$worktree_path" "$branch"
else
  echo "Worktree for $branch already exists at $worktree_path"
fi

echo "Done."