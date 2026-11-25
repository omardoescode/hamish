#!/usr/bin/env bash
# Claude Code WorktreeRemove hook. Stdin: JSON with "worktree_path".
set -euo pipefail

WT=$(jq -er '.worktree_path')
BRANCH=$(git -C "$WT" symbolic-ref --quiet --short HEAD || true)

git -C "$CLAUDE_PROJECT_DIR" worktree remove --force "$WT" >&2

if [[ -n "$BRANCH" ]]; then
  git -C "$CLAUDE_PROJECT_DIR" branch -D "$BRANCH" >&2
fi
