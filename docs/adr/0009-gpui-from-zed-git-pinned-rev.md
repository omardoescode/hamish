---
status: accepted
---
# GPUI from Zed git, pinned rev

GPUI comes from the Zed repository on GitHub, pinned to one commit (`rev` in the root `Cargo.toml`). ADR 0001 stays: Hamish is a Rust + GPUI app.

## Why

- On Linux, GPUI 0.2.2 draws text only in grayscale. The writer reads text all day, so soft text is a daily cost.
- The GPUI on Zed main has a wgpu renderer. It draws subpixel (LCD) text on Wayland and X11.
- crates.io has only GPUI 0.2.2. We do not use a fork, a local path, or an unofficial republish.

## What

- `gpui` and `gpui_platform` are git dependencies on `zed-industries/zed` at one `rev`. `gpui_platform` has the features `wayland`, `x11` and `font-kit`.
- `main` starts the app with `gpui_platform::application()`.
- The root `Cargo.toml` has a copy of the Zed `[patch.crates-io]` entries for the crates in our `Cargo.lock`, at the same revs as the pinned Zed commit.
- `deny.toml` keeps `unknown-git = "deny"`. Its `allow-git` list has only the git sources in the Linux graph: Zed, wgpu, the Zed forks, and the source of each copied patch.

## Rule

- Move the `rev` only for a fix or a feature that we need. Do it in its own PR.
- In that PR, copy the Zed patches again, update the `allow-git` list, and check each fact in `docs/gpui-notes.md` again.

## Consequences

- The release binary is 50% larger: 28.7 MB on GPUI 0.2.2, 43.1 MB on the pinned rev.
- The GPUI crates are not published. Each build fetches the Zed repository (cached by cargo).
- Zed main has breaking API changes. They reach us only when we move the `rev`.
- `#[gpui::test]` gives a test app with no text system. The app tests use the system text system through `crates/app/tests/system_text/mod.rs`.
