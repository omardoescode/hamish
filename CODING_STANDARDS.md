# Coding Standards (Rust)

- The guards below run in lefthook and CI. `--no-verify` skips them, but each rule still binds.
- `core` crate has no gpui dependency. All logic that you can test goes in `core`. `app` views stay thin.
- No `unwrap`/`expect` on user data or I/O outside tests (clippy `unwrap_used`, `expect_used` in non-test code).
- No lossy `as` casts. Use `From`/`TryFrom` (lint choice: `as_conversions` or `cast_*`, set in ticket 1).
- AI calls only through the AI adapter trait. The UI thread never waits on I/O (use the background executor).
- AI never changes text without a writer action (spec NFR).
- Names come from the glossary (CONTEXT.md).
- Tests: no tautological tests. Expected values come from the spec or hardcoded worked examples. Test at agreed seams only.
- Commits: no Co-Authored-By.
