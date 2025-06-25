#!/usr/bin/env bash
# Test script for Task 15: verify sandbox restrictions and allowances
set -euo pipefail

# Determine worktree root (script is placed under agentydragon/tasks)
worktree_root="$(cd "$(dirname "$0")"/.. && pwd)"

echo "Running sandbox tests in worktree: $worktree_root"

# Test write inside worktree
echo -n "Test: write inside worktree... "
if codex debug landlock --full-auto /usr/bin/env bash -c "touch '$worktree_root/inside_test'"; then
  echo "PASS"
else
  echo "FAIL" >&2
  exit 1
fi

# Test write inside TMPDIR
tmpdir=${TMPDIR:-/tmp}
echo -n "Test: write inside TMPDIR ($tmpdir)... "
if codex debug landlock --full-auto /usr/bin/env bash -c "touch '$tmpdir/tmp_test'"; then
  echo "PASS"
else
  echo "FAIL" >&2
  exit 1
fi

# Prepare external directory under HOME to test outside worktree/TMPDIR
external_dir="$HOME/sandbox_test_dir"
mkdir -p "$external_dir"
rm -f "$external_dir/outside_test"

echo -n "Test: write outside allowed paths ($external_dir)... "
if codex debug landlock --full-auto /usr/bin/env bash -c "touch '$external_dir/outside_test'"; then
  echo "FAIL: outside write succeeded" >&2
  exit 1
else
  echo "PASS"
fi
