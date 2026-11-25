# Where things are

## Layout

| Path | What |
|---|---|
| `crates/core/` | Package `ka-teb-core`: all testable logic (Document API). No gpui. |
| `crates/core/src/storage.rs` | Save/open: active text in `<name>.md`, all other state in `<name>.md.ka-teb.json` (schema `version`, `.md` SHA-256, re-anchor by text; ADR 0003). |
| `crates/core/src/title.rs` | `document_title`: the title at the top center and on the window (file name, else first line, else "Untitled"). |
| `crates/core/src/settings.rs` | Settings: defaults, limits, steps, `$XDG_CONFIG_HOME/ka-teb/settings.json` load/save. |
| `crates/core/src/theme.rs` | `ThemeName` (named themes, their order, labels, settings file names; `dark`/`light` load as Ka-teb/Ka-teb Light) and each `Palette` (`0xRRGGBB` per color role). |
| `crates/core/src/motion.rs` | `Effect`: the one Effect timing source (durations, progress, Reduced motion). |
| `crates/core/src/particles.rs` | Particle math of the Explosion and the Undo burst: sparks, flames, glow per progress. The app paints them (`crates/app/src/effects.rs`). |
| `crates/core/src/document.rs` | `Document`: text, Slots, Ghost ranges, Overflow, core undo. Slot rules for the views: `level_for` (Level of a new Slot), `slot_over` (innermost Slot over a range), `add_active_variant` / `create_active_slot` (one core step), `delete_variant` (gives the kept range when the Slot ends, past Article agreement). |
| `crates/core/src/edit_session.rs` | `EditSession`: the Document, the selection and the Undo steps as one module. Each change is one Undo step and the selection follows it; a change with no core step makes none. Undo and redo report the Slots to burst and fade in (`SlotChanges`). It keeps the Document `Caret` and makes its `CaretEdit`s. The Editor and all views change the Document only through it. |
| `crates/core/src/caret.rs` | `Caret`: selection, caret end, input method composed text over a plain text. Grapheme, word and line moves (`Motion`), the word at an offset (`word_at`), edits at the caret (`CaretEdit`), clamp to a changed text, UTF-16 ranges (`utf16_range`, `byte_range`), `follow_change`. No pixels. |
| `crates/core/src/edit_history.rs` | `EditHistory` (crate-private): groups core edits into Undo steps for `EditSession`; edits in the Document text and in Overflow never join. `EditKind`: Typing, Deleting, Other. |
| `crates/core/src/focus_zoom.rs` | `FocusZoom`: which paragraph shows zoomed and its scale over time. |
| `crates/core/src/sound.rs` | Cycle sounds and the Variant delete sound: which sound plays (`Switch::sound`) and its samples. |
| `crates/core/src/markdown.rs` | Markdown styles of the active text (pulldown-cmark, byte ranges). |
| `crates/core/src/ai.rs` | AI adapter trait `AiAdapter` (Variants, Lab checks, Trims), `AiRequest` (trigger: context menu or Quick fill; sentence, paragraph, existing Variants), `AiError`, `CancelToken` (stops a call; the backend kills `claude`); `Document::ai_request` / `apply_ai_result` (ADR 0004). |
| `crates/core/src/around.rs` | The sentence and paragraph around a text range. |
| `crates/core/src/ai/claude.rs` | `ClaudeBackend`: runs `claude -p` with the API credentials removed from its environment. Tested with a stub `claude` on `PATH` (`crates/core/tests/claude_backend.rs`). |
| `crates/app/src/ai.rs` | Global `Ai`: the adapter the app calls on the background executor. `main` sets `ClaudeBackend`; tests set a fake. `AiRun`: one AI call of a view (Lab checks, Trim, AI alternatives, Quick fill) with its `AiStatus`; a new start, `cancel`, or a drop cancels the call and drops its answer. `AiFailure`: typed reason, formatted only at render. |
| `crates/app/src/loading.rs` | The "??" loading dance of the empty panel row, for Quick fill and AI alternatives; still with Reduced motion. |
| `crates/app/src/caret.rs` | `CaretView`: the one GPUI adapter of `Caret` for the Editor and Overflow: caret key actions, clipboard, `EntityInputHandler` (macro `caret_input_handler!`), row moves from the painted text, caret quad (2 px) and scroll (edge fade margin). |
| `crates/app/src/effects.rs` | `EffectPlayer`: the particle Effects of one view (Editor, Alternatives panel); `PlayingEffect` (a core `Effect` at a text range); paints the Explosion and the Undo burst. |
| `crates/app/src/alternatives_panel.rs` | Alternatives panel: Variant rows, live preview of the empty row, Quick fill. |
| `crates/app/` | Package `app`: library (thin GPUI views, actions, key bindings, theme) + binary `ka-teb`. |
| `crates/app/src/editor.rs` | `Editor` entity: view state (hover, Markers, preview, spotlight, fade-in, Effects), its actions, `EditorEvent`s, `CaretView` adapter. |
| `crates/app/src/editor/painted_text.rs` | `PaintedText`: the text as last painted (`shape`, positions, offsets, rows, selection quads). Shared by the Editor, Overflow and `CaretView`. |
| `crates/app/src/editor/runs.rs` | Editor text runs: styled spans (title, Slot, Ghost, Trim cut, Markdown, dim), heading sizes, fade-in. |
| `crates/app/src/editor/pen.rs` | Slot underlines as pen strokes: rows, wobble seeded per Slot. |
| `crates/app/src/editor/element.rs` | `EditorElement`: layout, prepaint, paint; Markers, hover box, Lab marks, Slot hit test. |
| `crates/app/src/editor/focus_zoom.rs` | Focus zoom paint: the zoomed paragraph's glyphs at 1.02x (GPUI 0.2.2 has no text transform), quads moved over it, pointer mapped back for hit tests. |
| `crates/app/src/settings.rs` | Settings global: `use_settings`, `is_on` (how views read a setting), the start notice. |
| `crates/app/src/settings_modal.rs` | Settings modal (Ctrl+,): sections, fields, selects (theme, font), save on the background executor. |
| `crates/app/src/lab.rs` | `Lab` entity: LAB button, popover with Lab checks, Lab mark card. Holds the `Editor`; the Workspace places it and routes its actions. |
| `crates/app/src/trim.rs` | Trim part of the `Lab`: trim row, trim bar, Make the cuts, Walk through. |
| `crates/app/src/sounds.rs` | `SoundSink` and the `Sounds` global; `play` (checks the "Cycle sounds" setting); `DeviceSounds`: cpal/ALSA output on its own thread, silent without a device. |
| `crates/app/src/theme.rs` | Theme: GPUI colors from the `Palette` of the chosen theme, sizes from Settings. |
| `crates/app/assets/fonts/` | Bundled body font IBM Plex Mono (Regular, Bold, Italic, Bold Italic) and its license (`OFL.txt`). |
| `crates/app/assets/themes/ATTRIBUTION.md` | Source and license of each named theme palette. |
| `Cargo.toml` | Workspace, pinned `gpui`, `[workspace.lints]`, `[profile.dist]`. |
| `rust-toolchain.toml` | Pinned Rust version and components (local and CI). |
| `rustfmt.toml`, `clippy.toml` | Format and lint config (panic family allowed in tests; banned names, types, methods). |
| `deny.toml` | cargo-deny: banned crates, licenses, advisories, sources. |
| `.pedant.toml` | pedant style checks (nesting, `dead_code` suppression). |
| `.cargo-husky/hooks/` | Git hooks: pre-commit, commit-msg, pre-push. |
| `scripts/source-guards.pl` | Pre-commit source guards: file size (warning only), numbers and spec references in comments, glossary _Avoid_ words. Tests: `scripts/source-guards-test.sh`. |
| `.glossary-allow` | Known false hits of the glossary guard, each with a reason. |
| `crates/*/tests/` | Integration tests (public API, at the seams; `crates/app/tests/`: GPUI action dispatch with the GPUI test context). |
| `.claude/settings.json` | Claude Code project settings: attribution off, permissions, worktree hooks. |
| `.claude/hooks/` | WorktreeCreate / WorktreeRemove scripts; PostToolUse pedant check. |
| `.github/workflows/checks.yml` | CI: parallel jobs fmt, clippy, test (unit, integration, doc), guards (cargo-deny, cargo-shear, pedant). |
| `.github/workflows/release.yml` | Generated by cargo-dist (`dist generate`). Do not edit by hand. |
| `dist-workspace.toml` | cargo-dist config: targets, apt build libraries. |

## Commands

- Run the app: `cargo run -p app`
- All tests: `cargo test`
- Unit tests: `cargo test --workspace --lib --bins`
- Integration tests: `cargo test --workspace --test '*'`
- Doc tests: `cargo test --workspace --doc`
- One test: `cargo test -p ka-teb-core <test_name>` (substring match; add `-- --exact` for an exact name)
- Guards: `perl scripts/source-guards.pl`, `cargo deny check`, `cargo shear`, `pedant check crates` (owner table: CODING_STANDARDS.md)
- Source guard tests: `sh scripts/source-guards-test.sh`
- Release: push a tag `vX.Y.Z`. Regenerate CI after a dist config change: `dist generate`.

## Docs

- Glossary: [CONTEXT.md](../CONTEXT.md).
- Decisions: [docs/adr/README.md](adr/README.md).
- Code rules: [CODING_STANDARDS.md](../CODING_STANDARDS.md).
- GPUI 0.2.2 limits, with their source in the registry: [gpui-notes.md](gpui-notes.md).
- Agent rules: [CLAUDE.md](../CLAUDE.md).