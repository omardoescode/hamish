use std::time::Duration;

use app::Workspace;
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, TestAppContext, VisualTestContext, point, px,
};
use ka_teb_core::{Document, Level, LoadedSettings, Provenance, Settings, SlotId};

#[expect(clippy::unwrap_used, reason = "test fixture")]
fn open(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<Workspace>, SlotId, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let loaded = LoadedSettings {
        settings,
        problem: None,
    };
    cx.update(|cx| app::use_settings(None, loaded, cx));
    let mut document = Document::new("Title\nIt is a tension here.");
    let slot = document
        .create_slot(14..21, Level::Word, "struggle", Provenance::Writer)
        .unwrap();
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, slot, cx)
}

/// Puts the pointer on the Slot "tension" (bytes 14..21).
#[expect(clippy::unwrap_used, reason = "test fixture")]
fn hover_slot(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    let bounds = cx
        .update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.bounds_for_range(16..16, Bounds::default(), window, cx)
            })
        })
        .unwrap();
    let position = bounds.origin + point(px(3.), bounds.size.height / 2.);
    cx.simulate_mouse_move(position, None, Modifiers::none());
}

fn fading_in(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<(SlotId, f32)> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).fading_in(cx))
}

fn wait(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
}

#[gpui::test]
fn a_variant_switch_fades_the_new_text_in_over_150_ms(cx: &mut TestAppContext) {
    let (workspace, slot, cx) = open(cx, Settings::default());

    hover_slot(&workspace, cx);
    cx.simulate_keystrokes("down");

    let (fading, opacity) = fading_in(&workspace, cx).unwrap();
    assert_eq!(fading, slot);
    assert!((opacity - 0.2).abs() < 1e-3, "{opacity}");

    wait(cx, 75);
    let (_, opacity) = fading_in(&workspace, cx).unwrap();
    assert!((opacity - 0.6).abs() < 1e-3, "{opacity}");

    wait(cx, 75);
    assert_eq!(fading_in(&workspace, cx), None);
}

#[gpui::test]
fn undo_of_a_variant_switch_fades_the_old_text_in(cx: &mut TestAppContext) {
    let (workspace, slot, cx) = open(cx, Settings::default());

    hover_slot(&workspace, cx);
    cx.simulate_keystrokes("down");
    wait(cx, 200);
    cx.simulate_keystrokes("ctrl-z");

    assert_eq!(
        fading_in(&workspace, cx).map(|(fading, _)| fading),
        Some(slot)
    );
}

#[gpui::test]
fn typing_during_a_fade_goes_into_the_document(cx: &mut TestAppContext) {
    let (workspace, _, cx) = open(cx, Settings::default());

    hover_slot(&workspace, cx);
    cx.simulate_keystrokes("down");
    cx.simulate_input("A ");

    let text = cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .text()
            .to_owned()
    });
    assert_eq!(text, "A Title\nIt is a struggle here.");
    assert!(fading_in(&workspace, cx).is_some());
}

#[gpui::test]
fn with_reduced_motion_a_variant_switch_shows_at_once(cx: &mut TestAppContext) {
    let settings = Settings {
        reduced_motion: true,
        ..Settings::default()
    };
    let (workspace, _, cx) = open(cx, settings);

    hover_slot(&workspace, cx);
    cx.simulate_keystrokes("down");

    assert_eq!(fading_in(&workspace, cx), None);
}
