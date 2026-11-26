mod system_text;

use std::ops::Range;

use app::{AlternativesPanel, Workspace};
use gpui::{
    Bounds, Entity, EntityInputHandler, Focusable, Modifiers, MouseButton, Point, ScrollDelta,
    ScrollEvent, TestAppContext, VisualTestContext, point, px,
};
use hamish_core::{Document, Level, LoadedSettings, Provenance, Settings};

fn open(
    cx: &mut TestAppContext,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn panel(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<Entity<AlternativesPanel>> {
    cx.read(|cx| workspace.read(cx).alternatives_panel().cloned())
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<(String, bool)> {
    let panel = panel(workspace, cx).expect("the panel is open");
    cx.read(|cx| {
        panel
            .read(cx)
            .rows(cx)
            .into_iter()
            .map(|row| (row.text, row.active))
            .collect()
    })
}

fn row(text: &str, active: bool) -> (String, bool) {
    (text.to_owned(), active)
}

fn text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .text()
            .to_owned()
    })
}

/// "Title\nThe customer waits." ("customer" is 10..18).
fn customer_document() -> Document {
    Document::new("Title\nThe customer waits.")
}

/// Moves the caret into "customer".
const INTO_CUSTOMER: &str = "down right right right right right";

#[test]
fn ctrl_shift_a_on_a_word_opens_the_panel_with_the_word_as_the_original() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");

        assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);
        let level = panel(&workspace, cx).map(|panel| cx.read(|cx| panel.read(cx).level(cx)));
        assert_eq!(level, Some(Some(Level::Word)));
    });
}

#[test]
fn return_in_the_last_row_adds_a_variant_and_makes_it_active() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        cx.simulate_input("client");
        cx.simulate_keystrokes("enter");

        assert_eq!(
            rows(&workspace, cx),
            vec![row("customer", false), row("client", true)]
        );
        assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
    });
}

fn shown_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .shown_document()
            .text()
            .to_owned()
    })
}

#[test]
fn text_typed_in_the_empty_row_shows_in_the_document_in_place_of_the_slot_text() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, slot_document());
        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");

        cx.simulate_input("buyers");
        cx.simulate_keystrokes("backspace");

        assert_eq!(shown_text(&workspace, cx), "Title\nThe buyer waits.");
        assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
    });
}

#[test]
fn the_preview_ends_when_the_draft_is_added_or_the_panel_closes() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());
        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        cx.simulate_input("client");
        cx.simulate_keystrokes("enter");
        assert_eq!(shown_text(&workspace, cx), "Title\nThe client waits.");

        cx.simulate_input("patron");
        assert_eq!(shown_text(&workspace, cx), "Title\nThe patron waits.");
        cx.simulate_keystrokes("escape");

        assert_eq!(shown_text(&workspace, cx), "Title\nThe client waits.");
    });
}

#[test]
fn undo_and_redo_work_while_the_panel_has_focus() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());
        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        cx.simulate_input("client");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("patron");
        cx.simulate_keystrokes("enter");

        cx.simulate_keystrokes("ctrl-z");
        assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
        assert_eq!(
            rows(&workspace, cx),
            vec![row("customer", false), row("client", true)]
        );

        cx.simulate_keystrokes("ctrl-shift-z");
        assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
        assert!(panel(&workspace, cx).is_some());
    });
}

fn focused_on_editor(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    cx.update(|window, cx| editor.focus_handle(cx).is_focused(window))
}

#[test]
fn escape_closes_the_panel_and_the_editor_takes_the_keys_again() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a escape");

        assert!(panel(&workspace, cx).is_none());
        assert!(focused_on_editor(&workspace, cx));
    });
}

#[test]
fn escape_in_the_editor_closes_the_open_panel() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());
        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");

        let title = point_at(&workspace, cx, 2);
        cx.simulate_click(title, Modifiers::none());
        assert!(focused_on_editor(&workspace, cx));
        assert!(panel(&workspace, cx).is_some());

        cx.simulate_keystrokes("escape");

        assert!(panel(&workspace, cx).is_none());
    });
}

#[test]
fn the_panel_text_follows_editor_edits_before_it() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());
        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");

        let title = point_at(&workspace, cx, 2);
        cx.simulate_click(title, Modifiers::none());
        cx.simulate_input("xy");
        assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);

        cx.simulate_keystrokes("ctrl-z");
        assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
        assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);
    });
}

/// "Title\nThe customer waits." with a Word Slot on "customer": "customer", "client", "patron".
#[expect(clippy::expect_used, reason = "test fixture")]
fn slot_document() -> Document {
    let mut document = customer_document();
    let slot = document
        .create_slot(10..18, Level::Word, "client", Provenance::Writer)
        .expect("valid range");
    document
        .add_variant(slot, "patron", Provenance::Ai)
        .expect("the Slot exists");
    document
}

#[test]
fn up_and_down_in_the_panel_show_each_variant_in_the_document() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, slot_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a up");
        assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");

        cx.simulate_keystrokes("up");
        assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
        assert_eq!(
            rows(&workspace, cx),
            vec![
                row("customer", false),
                row("client", true),
                row("patron", false)
            ]
        );

        cx.simulate_keystrokes("down down");
        assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
    });
}

#[test]
fn ctrl_backspace_deletes_the_active_variant_and_the_original_shows() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, slot_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a up ctrl-backspace");

        assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
        assert_eq!(
            rows(&workspace, cx),
            vec![row("customer", true), row("client", false)]
        );
    });
}

#[test]
fn the_original_can_be_deleted_and_the_next_variant_shows() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, slot_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a up up up ctrl-backspace");

        assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
        assert_eq!(
            rows(&workspace, cx),
            vec![row("client", true), row("patron", false)]
        );
    });
}

#[test]
fn a_row_delete_plays_the_explosion_on_the_row_and_at_the_span() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, slot_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a up up ctrl-backspace");

        let panel = panel(&workspace, cx).expect("the panel is open");
        assert_eq!(cx.read(|cx| panel.read(cx).exploding_row()), Some(1));
        let effects = cx.read(|cx| workspace.read(cx).editor().read(cx).effects());
        assert_eq!(
            effects,
            vec![app::PlayingEffect {
                effect: hamish_core::Effect::Explosion,
                range: 10..16
            }]
        );
    });
}

#[expect(clippy::expect_used, reason = "test fixture")]
/// A window point in the middle of the row of byte `offset`, just right of it.
fn point_at(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    offset: usize,
) -> Point<gpui::Pixels> {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    let bounds = cx
        .update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.bounds_for_range(offset..offset, Bounds::default(), window, cx)
            })
        })
        .expect("the editor painted the text");
    bounds.origin + point(px(3.), bounds.size.height / 2.)
}

#[test]
fn a_click_on_a_slot_while_the_panel_is_open_shows_its_variants() {
    system_text::run(1, |cx| {
        let mut document = slot_document();
        assert!(
            document
                .create_slot(19..24, Level::Word, "stays", Provenance::Writer)
                .is_ok(),
            "valid range"
        );
        let (workspace, cx) = open(cx, document);

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        let waits = point_at(&workspace, cx, 21);
        cx.simulate_click(waits, Modifiers::none());
        assert_eq!(
            rows(&workspace, cx),
            vec![row("waits", true), row("stays", false)]
        );

        cx.simulate_keystrokes("up");
        assert_eq!(text(&workspace, cx), "Title\nThe customer stays.");
    });
}

fn bright(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Range<usize>> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).bright_paragraph())
}

#[test]
fn while_the_panel_is_open_only_the_paragraph_of_its_slot_stays_bright() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Document::new("Title\nThe customer waits.\nShe leaves."));
        assert_eq!(bright(&workspace, cx), None);

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        assert_eq!(bright(&workspace, cx), Some(6..25));

        cx.simulate_input("client");
        cx.simulate_keystrokes("enter up");
        assert_eq!(bright(&workspace, cx), Some(6..23));

        cx.simulate_keystrokes("escape");
        assert_eq!(bright(&workspace, cx), None);
    });
}

#[test]
fn the_text_column_does_not_move_when_the_panel_opens() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        let before = point_at(&workspace, cx, 13);
        cx.simulate_keystrokes("ctrl-shift-a");

        assert_eq!(point_at(&workspace, cx, 13), before);
    });
}

fn right_click(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, offset: usize) {
    let position = point_at(workspace, cx, offset);
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::none());
}

#[test]
fn the_context_menu_alternatives_row_opens_the_panel_on_the_word() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, customer_document());

        right_click(&workspace, cx, 13);
        let row_bounds = cx
            .debug_bounds("context-menu-row-0")
            .expect("the menu shows the Alternatives row");
        cx.simulate_click(row_bounds.center(), Modifiers::none());

        assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);
    });
}

#[test]
fn a_click_on_a_variant_row_shows_that_variant() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, slot_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        let row_bounds = cx
            .debug_bounds("alternatives-row-2")
            .expect("the panel shows the third Variant");
        cx.simulate_click(row_bounds.center(), Modifiers::none());

        assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
    });
}

#[test]
fn a_click_on_the_sentence_tab_shows_the_sentence_around_the_word() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Document::new("Title\nThe customer waits. She leaves."));

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        let tab = cx
            .debug_bounds("alternatives-tab-sentence")
            .expect("the panel shows the Sentence tab");
        cx.simulate_click(tab.center(), Modifiers::none());

        assert_eq!(rows(&workspace, cx), vec![row("The customer waits.", true)]);
        let level = panel(&workspace, cx).map(|panel| cx.read(|cx| panel.read(cx).level(cx)));
        assert_eq!(level, Some(Some(Level::Sentence)));
    });
}

#[test]
fn one_undo_in_the_editor_removes_a_variant_the_panel_added() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, slot_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        cx.simulate_input("buyer");
        cx.simulate_keystrokes("enter escape ctrl-z ctrl-shift-a");

        assert_eq!(
            rows(&workspace, cx),
            vec![
                row("customer", true),
                row("client", false),
                row("patron", false)
            ]
        );
    });
}
#[test]
fn with_reduced_motion_a_row_delete_plays_no_explosion() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, slot_document());
        let loaded = LoadedSettings {
            settings: Settings {
                reduced_motion: true,
                ..Settings::default()
            },
            problem: None,
        };
        cx.update(|_, cx| app::use_settings(None, loaded, cx));

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a up up ctrl-backspace");

        let panel = panel(&workspace, cx).expect("the panel is open");
        assert_eq!(cx.read(|cx| panel.read(cx).exploding_row()), None);
        let effects = cx.read(|cx| workspace.read(cx).editor().read(cx).effects());
        assert_eq!(effects, Vec::new());
    });
}

#[test]
fn a_long_variant_row_wraps_inside_the_panel() {
    system_text::run(1, |cx| {
        let sentence = "The customer waits at the counter for a long time while the clerk counts every coin twice.";
        let (_workspace, cx) = open(cx, Document::new(&format!("Title\n{sentence}")));

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        let tab = cx
            .debug_bounds("alternatives-tab-sentence")
            .expect("the panel shows the Sentence tab");
        cx.simulate_click(tab.center(), Modifiers::none());
        cx.run_until_parked();

        let panel = cx
            .debug_bounds("alternatives-panel")
            .expect("the panel is on screen");
        let row = cx
            .debug_bounds("alternatives-row-0")
            .expect("the panel shows the Original");
        let text = cx
            .debug_bounds("alternatives-row-text-0")
            .expect("the row shows its text");
        assert!(row.right() <= panel.right(), "row {row:?} panel {panel:?}");
        assert!(text.right() <= row.right(), "text {text:?} row {row:?}");
        assert!(
            row.size.height > px(46.),
            "the row has more than one line: {row:?}"
        );
    });
}

/// "customer" as a Word Slot with 40 long AI Variants after the Original: more rows than
/// the panel height.
#[expect(clippy::expect_used, reason = "test fixture")]
fn long_slot_document() -> Document {
    let mut document = customer_document();
    let long =
        "a patient buyer who stands at the counter and counts every coin twice before paying";
    let slot = document
        .create_slot(10..18, Level::Word, long, Provenance::Ai)
        .expect("valid range");
    for _ in 0..39 {
        document
            .add_variant(slot, long, Provenance::Ai)
            .expect("the Slot exists");
    }
    document
}

#[test]
fn the_wheel_scrolls_the_panel_rows() {
    system_text::run(1, |cx| {
        let (_workspace, cx) = open(cx, long_slot_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a");
        let panel = cx
            .debug_bounds("alternatives-panel")
            .expect("the panel is on screen");
        let before = cx
            .debug_bounds("alternatives-row-1")
            .expect("the panel shows the second Variant");
        let last = cx
            .debug_bounds("alternatives-row-41")
            .expect("the panel has the last row");
        assert!(
            last.bottom() > panel.bottom(),
            "the rows do not fit: {last:?}"
        );
        cx.simulate_mouse_move(before.center(), None, Modifiers::none());
        cx.simulate_event(ScrollEvent {
            position: before.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
            ..ScrollEvent::default()
        });
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();

        let after = cx
            .debug_bounds("alternatives-row-1")
            .expect("the panel shows the second Variant");
        assert_eq!(
            after.origin.y,
            before.origin.y - px(100.),
            "panel {panel:?}"
        );
    });
}

#[test]
fn typing_scrolls_the_last_row_into_the_panel() {
    system_text::run(1, |cx| {
        let (_workspace, cx) = open(cx, long_slot_document());

        cx.simulate_keystrokes(INTO_CUSTOMER);
        cx.simulate_keystrokes("ctrl-shift-a up");
        cx.simulate_input("buyer");
        cx.run_until_parked();

        let panel = cx
            .debug_bounds("alternatives-panel")
            .expect("the panel is on screen");
        let row = cx
            .debug_bounds("alternatives-row-41")
            .expect("the panel shows the last row");
        assert!(
            row.bottom() <= panel.bottom(),
            "row {row:?} panel {panel:?}"
        );
    });
}
