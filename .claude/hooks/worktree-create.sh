#!/usr/bin/env bash
# Claude Code WorktreeCreate hook. Stdin: JSON with "name".
# Stdout: only the worktree path (last line). Logs go to stderr.
set -euo pipefail

NAME=$(jq -er '.name')
DIR="$CLAUDE_PROJECT_DIR/.claude/worktrees/$NAME"

git -C "$CLAUDE_PROJECT_DIR" worktree add -b "worktree-$NAME" "$DIR" HEAD >&2

if [[ -d "$CLAUDE_PROJECT_DIR/target" ]]; then
  # Never fall back to a full copy: without btrfs CoW the worktree builds cold.
  cp -a --reflink=always "$CLAUDE_PROJECT_DIR/target" "$DIR/target" >&2 \
    || { echo "worktree-create: reflink copy of target/ failed; skipping it" >&2; rm -rf "$DIR/target"; }
fi

echo "$DIR"
