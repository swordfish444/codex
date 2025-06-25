#!/usr/bin/env bash
# launch_ready_to_merge.sh: open tmux panes for all tasks marked Ready to merge
set -euo pipefail

# Gather all tasks flagged Ready to merge by the status script
ready=$(agentydragon_task.py status \
  | sed -n -e '1,/^Ready to merge:/d' -e 's/^Ready to merge:[ ]*//')
if [ -z "$ready" ]; then
  echo "No tasks are Ready to merge."
  exit 0
fi

echo "Launching tasks: $ready"
agentydragon/tasks/create-task-worktree.sh --agent --tmux $ready
