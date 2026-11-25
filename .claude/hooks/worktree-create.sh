#!/usr/bin/env bash
# Claude Code WorktreeCreate hook. Stdin: JSON with "name".
# Stdout: only the worktree path (last line). Logs go to stderr.
set -euo pipefail

NAME=$(jq -er '.name')
# Real path, out of .claude/: a symlinked or protected path makes each agent edit ask for permission.
ROOT=$(realpath "$CLAUDE_PROJECT_DIR")
DIR="$ROOT/.worktrees/$NAME"

git -C "$ROOT" worktree add -b "worktree-$NAME" "$DIR" HEAD >&2

if [[ -d "$ROOT/target" ]]; then
  # Never fall back to a full copy: without btrfs CoW the worktree builds cold.
  cp -a --reflink=always "$ROOT/target" "$DIR/target" >&2 \
    || { echo "worktree-create: reflink copy of target/ failed; skipping it" >&2; rm -rf "$DIR/target"; }
fi

echo "$DIR"
