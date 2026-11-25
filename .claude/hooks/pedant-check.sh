#!/usr/bin/env bash
# PostToolUse hook (Edit|Write): pedant check on the edited .rs file.
# Based on pedant's examples/pedant-claude-code-hook.md. Exit 2 blocks and shows stderr to Claude.
set -euo pipefail

file_path=$(jq -r '.tool_input.file_path // empty')
[[ "$file_path" == *.rs && -f "$file_path" ]] || exit 0

if ! command -v pedant > /dev/null; then
  echo "PEDANT HOOK: pedant is not installed; style checks were NOT run (CODING_STANDARDS.md)" >&2
  exit 0
fi

output=$(pedant check -c "$CLAUDE_PROJECT_DIR/.pedant.toml" "$file_path" 2>&1) && code=0 || code=$?

if (( code == 1 )); then
  echo "PEDANT VIOLATIONS: fix before you continue" >&2
  echo "$output" >&2
  exit 2
fi
if (( code >= 2 )); then
  echo "PEDANT HOOK: pedant check failed (exit $code); style checks were NOT run" >&2
  echo "$output" >&2
fi
exit 0
