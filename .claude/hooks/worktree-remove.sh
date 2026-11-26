#!/usr/bin/env bash
# Claude Code WorktreeRemove hook. Stdin: JSON with "worktree_path".
set -euo pipefail

WT=$(jq -er '.worktree_path')
BRANCH=$(git -C "$WT" symbolic-ref --quiet --short HEAD || true)

ROOT=$(realpath "$CLAUDE_PROJECT_DIR")
git -C "$ROOT" worktree remove --force "$WT" >&2

if [[ -n "$BRANCH" ]]; then
  git -C "$ROOT" branch -D "$BRANCH" >&2
fi
