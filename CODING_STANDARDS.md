# Coding Standards (Rust)

- Git hooks: cargo-husky (dev-dependency of `core`, `user-hooks`; scripts in `.cargo-husky/hooks/`; installs on first `cargo test` in the main checkout, not in a linked worktree; worktrees share the main checkout's hooks). A changed hook script installs again only after `cargo clean -p cargo-husky && cargo test`. Tests for hook scripts: `scripts/test-hooks.sh`, run on the local machine only (not in CI). CI (`checks.yml`): fmt, clippy `-D warnings`, test on push and PR. Releases: cargo-dist, tag `v*.*.*`, Linux x86_64 only for now.
- The guards below run in git hooks (cargo-husky) and CI. `--no-verify` skips them, but each rule still binds.
- `core` crate has no gpui dependency. All logic that you can test goes in `core`. `app` views stay thin.
- No `unwrap`/`expect` on user data or I/O outside tests (clippy `unwrap_used`, `expect_used` in non-test code).
- No lossy `as` casts. Use `From`/`TryFrom`. Lint choice: clippy `cast_possible_truncation`, `cast_possible_wrap`, `cast_precision_loss`, `cast_sign_loss` (denied). A lossless `as` is allowed; for a lossy cast that is correct, use `#[expect(clippy::cast_..., reason = "...")]` on the smallest item.
- AI calls only through the AI adapter trait. The UI thread never waits on I/O (use the background executor).
- AI never changes text without a writer action (spec NFR).
- Names come from the glossary (CONTEXT.md).
- Tests: no tautological tests. Expected values come from the spec or hardcoded worked examples. Test at agreed seams only.
- Commits: no Co-Authored-By.