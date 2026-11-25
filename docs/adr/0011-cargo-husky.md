---
status: accepted
---
# cargo-husky for git hooks

Git hooks come from cargo-husky, a dev-dependency (`user-hooks` feature) with scripts in `.cargo-husky/hooks/`. The first `cargo test` installs them, so a clone needs no OS tool. Hooks: pre-commit fmt + clippy, commit-msg rejects Co-Authored-By, pre-push test.

## Considered Options
- lefthook: OS-level install, not on crates.io.
- `.githooks/` + `core.hooksPath`: needs a manual setup step per clone.
- prek: needs `cargo install` per machine.

## Consequences
cargo-husky has had no release since 2020. A changed hook script installs again only after `target/` is cleaned (verify in ticket 1).
