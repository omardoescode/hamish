#!/usr/bin/env bash
# Tests for .cargo-husky/hooks/commit-msg and .claude/hooks/worktree-*.sh.
# Run: scripts/test-hooks.sh
set -euo pipefail

ROOT=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
mkdir -p "$ROOT/target"
TMP=$(mktemp -d "$ROOT/target/hook-tests.XXXXXX")
trap 'git -C "$TMP/repo" worktree prune 2>/dev/null; rm -rf "$TMP"' EXIT

failures=0
pass() { echo "ok   $1"; }
fail() { echo "FAIL $1"; failures=$((failures + 1)); }

commit_msg() {
  printf '%s\n' "$1" > "$TMP/msg"
  "$ROOT/.cargo-husky/hooks/commit-msg" "$TMP/msg" > /dev/null 2>&1
}

commit_msg "feat: add Slot" && pass "commit-msg accepts a clean message" || fail "commit-msg accepts a clean message"
! commit_msg $'feat: add Slot\n\nCo-Authored-By: Bot <bot@example.com>' && pass "commit-msg rejects Co-Authored-By" || fail "commit-msg rejects Co-Authored-By"
! commit_msg $'feat: add Slot\n\nco-authored-by: Bot <bot@example.com>' && pass "commit-msg rejects lowercase co-authored-by" || fail "commit-msg rejects lowercase co-authored-by"
! commit_msg $'feat: add Slot\n\nCO-AUTHORED-BY:Bot' && pass "commit-msg rejects uppercase, no space" || fail "commit-msg rejects uppercase, no space"
commit_msg $'docs: explain why we ban the co-authored-by trailer' && pass "commit-msg accepts the words in a sentence" || fail "commit-msg accepts the words in a sentence"

PROJECT="$TMP/repo"
git init -q -b main "$PROJECT"
git -C "$PROJECT" -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
mkdir -p "$PROJECT/target/debug"
echo built > "$PROJECT/target/debug/artifact"

out=$(echo '{"hook_event_name":"WorktreeCreate","cwd":"'"$PROJECT"'","name":"bold-oak-a3f2"}' \
  | CLAUDE_PROJECT_DIR="$PROJECT" "$ROOT/.claude/hooks/worktree-create.sh" 2> "$TMP/create.err") \
  && pass "worktree-create exits 0" || fail "worktree-create exits 0 ($(cat "$TMP/create.err"))"
WT="$PROJECT/.claude/worktrees/bold-oak-a3f2"
[[ "$(printf '%s\n' "$out" | tail -n 1)" == "$WT" ]] && pass "worktree-create prints the path last" || fail "worktree-create prints the path last (got: $out)"
[[ "$(git -C "$WT" rev-parse --abbrev-ref HEAD 2>/dev/null)" == "worktree-bold-oak-a3f2" ]] && pass "worktree is on branch worktree-<name>" || fail "worktree is on branch worktree-<name>"
if [[ "$(stat -f -c %T "$PROJECT")" == btrfs ]]; then
  [[ "$(cat "$WT/target/debug/artifact" 2>/dev/null)" == built ]] && pass "worktree-create reflinks target/" || fail "worktree-create reflinks target/"
else
  [[ ! -e "$WT/target" ]] && pass "worktree-create skips target/ without reflink support" || fail "worktree-create skips target/ without reflink support"
fi

echo '{"hook_event_name":"WorktreeRemove","cwd":"'"$PROJECT"'","worktree_path":"'"$WT"'"}' \
  | CLAUDE_PROJECT_DIR="$PROJECT" "$ROOT/.claude/hooks/worktree-remove.sh" > /dev/null 2> "$TMP/remove.err" \
  && pass "worktree-remove exits 0" || fail "worktree-remove exits 0 ($(cat "$TMP/remove.err"))"
[[ ! -e "$WT" ]] && pass "worktree-remove deletes the directory" || fail "worktree-remove deletes the directory"
! git -C "$PROJECT" rev-parse --verify -q refs/heads/worktree-bold-oak-a3f2 > /dev/null && pass "worktree-remove deletes the branch" || fail "worktree-remove deletes the branch"

if (( failures > 0 )); then
  echo "$failures test(s) failed"
  exit 1
fi
echo "all hook tests passed"
