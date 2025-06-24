#!/usr/bin/env bash
#
# review-unmerged-task-branches.sh
#
# Launch a Codex agent to review all task branches not yet merged and facilitate merging completed tasks.

set -euo pipefail

codex "Your task is to review all branches matching 'agentydragon/[0-9][0-9]-*' that are not merged into the 'agentydragon' branch.\
For each branch: determine its slug and run 'create-task-worktree.sh --agent <slug>' in agentydragon/tasks/. Work in the generated worktree.\
Review the task's Markdown and any code changes to ensure the task is complete, documented, and Status/Implementation sections are accurate.\
After reviewing each branch, ask me if I should merge it into 'agentydragon'; if I approve, perform the merge and clean up the worktree and branch.\
At the end, present a summary of any branches still needing further work."