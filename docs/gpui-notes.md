# GPUI 0.2.2 notes

Verified limits of the pinned `gpui = "=0.2.2"`. Read these before you design around GPUI.
Paths are in `~/.cargo/registry/src/index.crates.io-*/gpui-0.2.2/`. Check a fact again when the pin changes.

| Fact | Source | What we do |
|---|---|---|
| No text or quad transform. `paint_glyph` sets `transformation: TransformationMatrix::unit()` on each sprite. `PaintQuad` has no transform field. | `src/window.rs`: `paint_glyph` (line 2948, unit matrix at 2996), `struct PaintQuad` (line 5009) | Focus zoom paints the glyphs at the zoomed size and moves the quads (`crates/app/src/editor/focus_zoom.rs`). |
| No OS reduced-motion value. The `Platform` trait gives only `window_appearance` (light or dark). | `src/platform.rs`: `trait Platform` (line 163), `window_appearance` (line 212) | Reduced motion is a setting, off by default (`crates/core/src/settings.rs`). |
| `all_font_names()` adds the fallback font stack (`.ZedMono`, `.ZedSans`, `Helvetica`, ...) and `.SystemUIFont` to the OS fonts. A name in the list is not always an installed font. | `src/text_system.rs`: `all_font_names` (line 90), `fallback_font_stack` (line 73) | The body font list in the Settings modal (`crates/app/src/settings_modal.rs`). |
| The test context does not draw after `run_until_parked`. It only runs the executor. The test window ignores `on_request_frame`, so `request_animation_frame` never draws. | `src/app/test_context.rs`: `VisualTestContext::run_until_parked` (line 696); `src/platform/test/window.rs`: `on_request_frame` (line 235) | Call `window.refresh()` before `run_until_parked` to paint a frame (`crates/app/tests/focus_zoom.rs`, `wait`). |
| `debug_bounds` keeps the bounds of old frames. `Frame::clear` does not clear the map, so an element that no longer paints still has bounds. | `src/window.rs`: field `debug_bounds` (line 682), `Frame::clear` (line 740); `src/app/test_context.rs`: `debug_bounds` (line 809) | Do not use `debug_bounds` to prove that an element is gone. Check the view state. |
