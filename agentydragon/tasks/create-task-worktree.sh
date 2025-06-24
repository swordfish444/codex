#!/usr/bin/env bash
#
# create-task-worktree.sh
#
# Create or reuse a git worktree for a specific task branch under agentydragon/tasks/.worktrees.
# Usage: create-task-worktree.sh [-a|--agent] [-t|--tmux] <task-id>-<task-slug> [<more-task-ids>...]

set -euo pipefail

agent_mode=false
tmux_mode=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    -a|--agent)
      agent_mode=true
      shift
      ;;
    -t|--tmux)
      tmux_mode=true
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [-a|--agent] [-t|--tmux] <task-id>-<task-slug> [<more-task-ids>...]"
      echo "  -a, --agent    after creating/reusing, launch a codex agent in the task workspace"
      echo "  -t, --tmux     open multiple tasks in tmux session (implies --agent)"
      exit 0
      ;;
    *)
      break
      ;;
  esac
done

# Validate number of task arguments based on mode
if [ "$tmux_mode" = true ]; then
  if [ "$#" -lt 1 ]; then
    echo "Usage: $0 [-a|--agent] [-t|--tmux] <task-id>-<task-slug> [<more-task-ids>...]"
    exit 1
  fi
else
  if [ "$#" -ne 1 ]; then
    echo "Usage: $0 [-a|--agent] [-t|--tmux] <task-id>-<task-slug> [<more-task-ids>...]"
    exit 1
  fi
fi

# Capture raw input so we can accept just a two-digit task ID
task_inputs=("$@")

# If tmux mode, batch-create worktrees and launch in tmux
if [ "$tmux_mode" = true ]; then
  # Implicitly enable agent mode in tmux panes
  agent_mode=true
  # Build a unique session name from task IDs
  session="agentydragon-${task_inputs[*]// /_}"
  cmd="$0"
  # First pane: first task
  first="${task_inputs[0]}"
  pane_cmd="$cmd${agent_mode:+ -a} $first"
  tmux new-session -d -s "$session" "$pane_cmd"
  # Split for remaining tasks
  for task in "${task_inputs[@]:1}"; do
    tmux split-window -v "$cmd${agent_mode:+ -a} $task"
  done
  tmux select-layout tiled
  tmux attach -t "$session"
  exit 0
fi

task_input="${task_inputs[0]}"

# Determine repository root and tasks directory
repo_root=$(git rev-parse --show-toplevel)
tasks_dir="$repo_root/agentydragon/tasks"

# If given only a two-digit ID, resolve to the full task slug
if [[ "$task_input" =~ ^[0-9]{2}$ ]]; then
  matches=( "$tasks_dir/${task_input}-"*.md )
  if [ "${#matches[@]}" -eq 1 ]; then
    task_slug="$(basename "${matches[0]}" .md)"
    echo "Resolved task ID '$task_input' to slug '$task_slug'"
  else
    echo "Error: expected exactly one task file matching '${task_input}-*.md', found ${#matches[@]}" >&2
    exit 1
  fi
else
  task_slug="$task_input"
fi
branch="agentydragon/$task_slug"
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