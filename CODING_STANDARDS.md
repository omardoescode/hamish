# Coding Standards (Rust)

- Git hooks: cargo-husky (dev-dependency; installs on first `cargo test`). CI (`checks.yml`): fmt, clippy `-D warnings`, test on push and PR. Releases: cargo-dist, tag `v*.*.*`, Linux x86_64 only for now.
- The guards below run in git hooks (cargo-husky) and CI. `--no-verify` skips them, but each rule still binds.
- `core` crate has no gpui dependency. All logic that you can test goes in `core`. `app` views stay thin.
- No `unwrap`/`expect` on user data or I/O outside tests (clippy `unwrap_used`, `expect_used` in non-test code).
- No lossy `as` casts. Use `From`/`TryFrom` (lint choice: `as_conversions` or `cast_*`, set in ticket 1).
- AI calls only through the AI adapter trait. The UI thread never waits on I/O (use the background executor).
- AI never changes text without a writer action (spec NFR).
- Names come from the glossary (CONTEXT.md).
- Tests: no tautological tests. Expected values come from the spec or hardcoded worked examples. Test at agreed seams only.
- Commits: no Co-Authored-By.