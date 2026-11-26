mod system_text;

use std::ops::Range;
use std::time::Duration;

use app::Workspace;
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, Pixels, TestAppContext, VisualTestContext,
    point, px,
};
use hamish_core::{Document, Level, LoadedSettings, Provenance, Settings};

/// "Title" is 0..5, the long paragraph 6..60, "Short." 61..67.
const TEXT: &str = "Title\nabcdefghij abcdefghij abcdefghij abcdefghij abcdefghij\nShort.";

fn open(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    open_document(cx, settings, Document::new(TEXT))
}

fn open_document(
    cx: &mut TestAppContext,
    settings: Settings,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let loaded = LoadedSettings {
        settings,
        problem: None,
    };
    cx.update(|cx| app::use_settings(None, loaded, cx));
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn zoomed(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<(Range<usize>, f32)> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).focus_zoom(cx))
}

/// Lets `millis` pass and paints the next frame, as `request_animation_frame` does.
fn wait(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn near(value: f32, expected: f32) -> bool {
    (value - expected).abs() < 1e-4
}

#[expect(clippy::unwrap_used, reason = "test fixture")]
fn caret_bounds(
    workspace: &Entity<Workspace>,
    offset: usize,
    cx: &mut VisualTestContext,
) -> Bounds<Pixels> {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.bounds_for_range(offset..offset, Bounds::default(), window, cx)
        })
    })
    .unwrap()
}

fn caret(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range<usize> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).selection())
}

#[test]
fn the_paragraph_with_the_caret_zooms_to_1_02_over_150_ms() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());

        cx.simulate_keystrokes("down");
        wait(cx, 75);
        let half = zoomed(&workspace, cx);
        wait(cx, 75);
        let full = zoomed(&workspace, cx);

        assert!(
            half.iter()
                .any(|(range, scale)| *range == (6..60) && near(*scale, 1.01))
        );
        assert_eq!(full.len(), 1);
        assert_eq!(full[0].0, 6..60);
        assert!(near(full[0].1, 1.02));
    });
}

#[test]
fn the_zoom_moves_with_the_caret() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());
        cx.simulate_keystrokes("down");
        wait(cx, 200);

        cx.simulate_keystrokes("down");
        wait(cx, 200);

        let full = zoomed(&workspace, cx);
        assert_eq!(full.len(), 1);
        assert_eq!(full[0].0, 61..67);
    });
}

#[test]
fn with_reduced_motion_the_zoom_is_at_once() {
    system_text::run(1, |cx| {
        let settings = Settings {
            reduced_motion: true,
            ..Settings::default()
        };
        let (workspace, cx) = open(cx, settings);

        cx.simulate_keystrokes("down");

        let full = zoomed(&workspace, cx);
        assert_eq!(full.len(), 1);
        assert!(near(full[0].1, 1.02));
    });
}

#[test]
fn with_the_setting_off_nothing_zooms() {
    system_text::run(1, |cx| {
        let settings = Settings {
            focus_zoom: false,
            ..Settings::default()
        };
        let (workspace, cx) = open(cx, settings);

        cx.simulate_keystrokes("down");
        wait(cx, 200);

        assert_eq!(zoomed(&workspace, cx), Vec::new());
    });
}

#[test]
fn nothing_zooms_while_the_alternatives_panel_dims_the_text() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());

        cx.simulate_keystrokes("down right right ctrl-shift-a");
        wait(cx, 200);

        assert!(workspace.read_with(cx, |workspace, _| workspace.alternatives_panel().is_some()));
        assert_eq!(zoomed(&workspace, cx), Vec::new());
    });
}

#[test]
fn the_zoomed_paragraph_shows_1_02_times_wider() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());
        cx.simulate_keystrokes("down");
        let normal =
            caret_bounds(&workspace, 56, cx).origin.x - caret_bounds(&workspace, 10, cx).origin.x;

        wait(cx, 200);
        let zoomed =
            caret_bounds(&workspace, 56, cx).origin.x - caret_bounds(&workspace, 10, cx).origin.x;

        assert!(near(zoomed / normal, 1.02), "{normal:?} {zoomed:?}");
    });
}

#[test]
fn a_click_on_the_zoomed_text_puts_the_caret_at_the_glyph_under_the_pointer() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());
        cx.simulate_keystrokes("down");
        wait(cx, 200);

        for offset in [7, 58] {
            let bounds = caret_bounds(&workspace, offset, cx);
            let position = bounds.origin + point(px(1.), bounds.size.height / 2.);
            cx.simulate_click(position, Modifiers::none());
            assert_eq!(caret(&workspace, cx), offset..offset);
        }
    });
}

#[test]
fn the_pointer_on_the_zoomed_end_of_a_slot_hovers_it() {
    system_text::run(1, |cx| {
        let mut document = Document::new(TEXT);
        let slot = document
            .create_slot(50..60, Level::Word, "klmnopqrst", Provenance::Writer)
            .unwrap();
        let (workspace, cx) = open_document(cx, Settings::default(), document);
        cx.simulate_keystrokes("down");
        wait(cx, 200);

        let bounds = caret_bounds(&workspace, 60, cx);
        let position = bounds.origin + point(px(-1.), bounds.size.height / 2.);
        cx.simulate_mouse_move(position, None, Modifiers::none());

        let hovered = cx.read(|cx| workspace.read(cx).editor().read(cx).hovered_slot());
        assert_eq!(hovered, Some(slot));
    });
}
