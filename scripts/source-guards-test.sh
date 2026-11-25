#!/bin/sh
# Runs scripts/source-guards.pl on small sample trees and checks its exit status.
set -u
guards="$(cd "$(dirname "$0")" && pwd)/source-guards.pl"
failures=0

new_tree() {
  tree=$(mktemp -d)
  mkdir -p "$tree/crates/core/src"
  printf '# Glossary\n\n**Caret**:\nThe insertion point.\n_Avoid_: cursor\n\n**Undo step**:\nOne undo.\n_Avoid_: History entry, undo group\n' > "$tree/CONTEXT.md"
  : > "$tree/.glossary-allow"
}

expect() {
  want=$1
  name=$2
  perl "$guards" "$tree" > "$tree.out" 2>&1
  got=$?
  if [ "$got" -ne "$want" ]; then
    echo "FAIL: $name: exit $got, want $want" >&2
    cat "$tree.out" >&2
    failures=$((failures + 1))
  fi
  rm -rf "$tree" "$tree.out"
}

lines() {
  awk -v n="$1" 'BEGIN { for (i = 0; i < n; i++) print "fn f() {}" }'
}

new_tree; lines 1000 > "$tree/crates/core/src/a.rs"
expect 0 "a file with 1000 lines passes"

new_tree; lines 1001 > "$tree/crates/core/src/a.rs"
expect 1 "a file with 1001 lines fails"

new_tree; printf '// Rule FR-1 holds.\nfn f() {}\n' > "$tree/crates/core/src/a.rs"
expect 1 "an FR number in a // comment fails"

new_tree; printf '/// Fixed in #12.\nfn f() {}\n' > "$tree/crates/core/src/a.rs"
expect 1 "an issue reference in a /// comment fails"

new_tree; printf '//! Holds the NFR rules.\n' > "$tree/crates/core/src/a.rs"
expect 1 "NFR in a //! comment fails"

new_tree; printf '// The spec asks for this.\nfn f() {}\n' > "$tree/crates/core/src/a.rs"
expect 1 "a spec reference in a comment fails"

new_tree; printf 'fn f() -> (&str, &str) {\n    ("FR-1 and #12", r#"see issue #12"#)\n}\n' > "$tree/crates/core/src/a.rs"
expect 0 "FR-1 and #12 in string literals pass"

new_tree; printf '// Rule of ADR 0004.\n#[derive(Debug)]\nstruct S;\n' > "$tree/crates/core/src/a.rs"
expect 0 "an ADR number and an attribute pass"

new_tree; printf 'fn move_cursor() {}\n' > "$tree/crates/core/src/a.rs"
expect 1 "an _Avoid_ word in an identifier fails"

new_tree; printf 'struct UndoGroup;\n' > "$tree/crates/core/src/a.rs"
expect 1 "an _Avoid_ phrase in a CamelCase identifier fails"

new_tree; printf '// The cursor moves.\nfn f() {}\n' > "$tree/crates/core/src/a.rs"
expect 1 "an _Avoid_ word in a comment fails"

new_tree; printf 'fn f() -> &str {\n    "cursor"\n}\nfn recursor() {}\n' > "$tree/crates/core/src/a.rs"
expect 0 "an _Avoid_ word in a string or inside a longer word passes"

new_tree; printf 'fn f(d: Div) -> Div {\n    d.cursor(CursorStyle::IBeam)\n}\n' > "$tree/crates/core/src/a.rs"
printf '.cursor(CursorStyle  GPUI method and type name.\n' > "$tree/.glossary-allow"
expect 0 "an allowlisted hit passes"

new_tree; printf 'fn f(d: Div) -> Div {\n    d.cursor(CursorStyle::IBeam)\n}\n' > "$tree/crates/core/src/a.rs"
printf '.cursor(CursorStyle\n' > "$tree/.glossary-allow"
expect 1 "an allowlist entry with no reason fails"

if [ "$failures" -ne 0 ]; then
  echo "$failures source guard test(s) failed." >&2
  exit 1
fi
echo "source guard tests passed."
