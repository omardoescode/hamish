# Coding Standards (Rust)

- Git hooks: cargo-husky (dev-dependency of `core`, `user-hooks`; scripts in `.cargo-husky/hooks/`; installs on first `cargo test` in the main checkout, not in a linked worktree; worktrees share the main checkout's hooks). A changed hook script installs again only after `cargo clean -p cargo-husky && cargo test`. CI (`checks.yml`): fmt, clippy `-D warnings`, test, guards (cargo-deny, cargo-shear, pedant) on push to `main` and on non-draft PRs. Toolchain: `rust-toolchain.toml` (pinned stable, with rustfmt and clippy), the same for local and CI. Release workflow: version tags only. Releases: cargo-dist, tag `v*.*.*`, Linux x86_64 only for now.
- The guards below run in git hooks (cargo-husky) and CI. `--no-verify` skips them, but each rule still binds.
- `core` crate has no gpui dependency. All logic that you can test goes in `core`. `app` views stay thin.
- No `unwrap`/`expect` on user data or I/O outside tests (clippy `unwrap_used`, `expect_used` in non-test code).
- No `as` casts (clippy `as_conversions`, denied). Use `From`/`TryFrom`. For a cast that is correct and has no `From`/`TryFrom`, use `#[expect(clippy::as_conversions, reason = "...")]` on the smallest item.
- AI calls only through the AI adapter trait. The UI thread never waits on I/O (use the background executor).
- AI never changes text without a writer action (spec NFR).
- Names come from the glossary (CONTEXT.md).
- No Arabic text anywhere in the repo (code, tests, docs). For non-ASCII test text use other scripts (e.g. Greek).
- Unit tests: `#[cfg(test)] mod tests` in the same file; they test private logic of one module. Run: `cargo test --workspace --lib --bins`.
- Integration tests: `crates/<crate>/tests/*.rs`; public API only, at the agreed seams (Document API, persistence round trip, AI adapter, GPUI action dispatch). Run: `cargo test --workspace --test '*'`.
- Doc tests: examples in doc comments. Run: `cargo test --workspace --doc`.
- Architecture: crate boundaries only (`core` does not list gpui in Cargo.toml; `pub(crate)` inside a crate). No architecture tests or tools.
- Tests: no tautological tests. Expected values come from the spec or hardcoded worked examples. Test at agreed seams only.
- Commits: no Co-Authored-By.

## Anti-slop guards

Each concern has one owner tool. When two tools can check the same thing, the first tool in this order owns it: rustfmt > clippy lints > clippy.toml > cargo-deny > cargo-shear > pedant. The other tool has that check off.

| Concern | Owner | Where |
|---|---|---|
| Format | rustfmt | `rustfmt.toml` |
| Pedantic code quality (includes lossy `cast_*`) | clippy `pedantic` (warn; `-D warnings` makes it an error) | `Cargo.toml` `[workspace.lints.clippy]` |
| `as` casts | clippy `as_conversions` | same |
| Panics: `unwrap`, `expect`, `panic!`, indexing/slicing, `todo!`, `unimplemented!`, `exit` | clippy restriction lints (denied; tests may panic, `clippy.toml` `allow-*-in-tests`) | same |
| Debug output: `dbg!`, `print!`/`println!`, `eprint!`/`eprintln!` | clippy `dbg_macro`, `print_stdout`, `print_stderr` | same |
| Lint suppression without a reason | clippy `allow_attributes_without_reason` | same |
| Discarded `#[must_use]` values, `map_err(\|_\| ...)`, wildcard enum arms, `.clone()` on `Rc`/`Arc` | clippy `let_underscore_must_use`, `map_err_ignore`, `wildcard_enum_match_arm`, `clone_on_ref_ptr` | same |
| Generic names (`data`, `helper`, `util`, `utils`, `manager`, `handler`, `stuff`, `temp`, `foo`, `baz`, `quux`) | clippy `disallowed-names` | `clippy.toml` |
| Untyped values (`serde_json::Value`, `std::any::Any`) | clippy `disallowed-types`; persistence/AI boundary parsing only, with a scoped `#[expect(clippy::disallowed_types, reason = "...")]` | `clippy.toml` |
| Process execution (`std::process::Command::new`) | clippy `disallowed-methods`; AI backend module only, with a scoped `#[expect(clippy::disallowed_methods, reason = "...")]` | `clippy.toml` |
| Mock crates (`mockall`, `mockito`, `faux`, `mry`): test at real seams with fakes or stubs | cargo-deny `[bans]` | `deny.toml` |
| Licenses (allowlist), security advisories, yanked crates, unknown sources, wildcard versions | cargo-deny | `deny.toml` |
| Duplicate crate versions | cargo-deny (warn only) | `deny.toml` |
| Unused dependencies | cargo-shear (cargo's own `unused_dependencies` lint is off: `[workspace.lints.cargo]`) | `[package.metadata.cargo-shear]` for false positives |
| Nesting: depth > 3, nested `if`, nested `match`, `if` in `match`, `match` in `if`, long `else if` chain | pedant `check` (defaults) | `.pedant.toml` |
| `dead_code` suppression (`allow` or `expect`) | pedant `forbidden-attribute` (clippy only asks for a reason) | `.pedant.toml` |

Not used, because a tool earlier in the order owns the concern: pedant `let-underscore-result` (clippy `let_underscore_must_use`), `generic-naming` (clippy `disallowed-names`), `forbidden-call`/`forbidden-macro` (clippy restriction lints). Not used, because architecture is crate boundaries only: pedant capability gate (`core` has no network).

Where each guard runs:

- pre-commit: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
- pre-push: `cargo test`, `cargo deny check`, `cargo shear`, `pedant check crates`. Each is under 3 s on this workspace, so they run before push. The hook stops if a tool is not installed.
- CI: all of the above. Job `guards`: cargo-deny and cargo-shear from `taiki-e/install-action` (prebuilt). pedant has no prebuilt binary, so CI compiles it one time and rust-cache keeps it.
- Claude Code: PostToolUse hook on Edit/Write (`.claude/hooks/pedant-check.sh`) runs `pedant check` on the edited `.rs` file. A violation blocks (exit 2). A missing or broken pedant gives a warning, no block.

Install the tools (versions as in CI): `cargo install cargo-deny@0.22.1 cargo-shear@1.14.1 pedant@0.31.1 --locked` (or `cargo binstall` for cargo-deny and cargo-shear).
