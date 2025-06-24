#!/usr/bin/env bash
#
# create-task-worktree.sh
#
# Create or reuse a git worktree for a specific task branch under agentydragon/tasks/.worktrees.
# Usage: create-task-worktree.sh <task-id>-<task-slug>

set -euo pipefail

agent_mode=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    -a|--agent)
      agent_mode=true
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [-a|--agent] <task-id>-<task-slug>"
      echo "  -a, --agent    after creating/reusing, launch a codex agent in the task workspace"
      exit 0
      ;;
    *)
      break
      ;;
  esac
done

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 [-a|--agent] <task-id>-<task-slug>"
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
  echo "Creating branch $branch from agentydragon branch..."
  git branch --track "$branch" agentydragon
fi

# Create worktree if it does not exist
if [ ! -d "$worktree_path" ]; then
  echo "Creating worktree for $branch at $worktree_path"
  git worktree add "$worktree_path" "$branch"

else
  echo "Worktree for $branch already exists at $worktree_path"
fi

echo "Done."

if [ "$agent_mode" = true ]; then
  echo "Launching codex agent for task $task_slug in $worktree_path"
  cd "$worktree_path"
  codex "Read the task definition in agentydragon/tasks/$task_slug.md and update its **Status** and **Implementation** sections to make progress on the task. Continue editing the file until the task is complete."
fi