use std::ops::Range;

use app::{AlternativesPanel, Workspace};
use gpui::{
    Bounds, Entity, EntityInputHandler, Focusable, Modifiers, MouseButton, Point, TestAppContext,
    VisualTestContext, point, px,
};
use ka_teb_core::{Document, Level, Provenance};

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

/// Moves the cursor into "customer".
const INTO_CUSTOMER: &str = "down right right right right right";

#[gpui::test]
fn ctrl_shift_a_on_a_word_opens_the_panel_with_the_word_as_the_original(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a");

    assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);
    let level = panel(&workspace, cx).map(|panel| cx.read(|cx| panel.read(cx).level(cx)));
    assert_eq!(level, Some(Some(Level::Word)));
}

#[gpui::test]
fn return_in_the_last_row_adds_a_variant_and_makes_it_active(cx: &mut TestAppContext) {
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

#[gpui::test]
fn text_typed_in_the_empty_row_shows_in_the_document_in_place_of_the_slot_text(
    cx: &mut TestAppContext,
) {
    let (workspace, cx) = open(cx, slot_document());
    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a");

    cx.simulate_input("buyers");
    cx.simulate_keystrokes("backspace");

    assert_eq!(shown_text(&workspace, cx), "Title\nThe buyer waits.");
    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
}

#[gpui::test]
fn the_preview_ends_when_the_draft_is_added_or_the_panel_closes(cx: &mut TestAppContext) {
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
}

#[gpui::test]
fn after_return_a_new_empty_row_takes_the_next_variant(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());
    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a");
    cx.simulate_input("client");
    cx.simulate_keystrokes("enter");

    cx.simulate_input("patron");
    cx.simulate_keystrokes("enter");

    assert_eq!(
        rows(&workspace, cx),
        vec![
            row("customer", false),
            row("client", false),
            row("patron", true)
        ]
    );
    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
}

#[gpui::test]
fn one_undo_removes_a_variant_added_with_return(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());
    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a");
    cx.simulate_input("client");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("patron");
    cx.simulate_keystrokes("enter");

    cx.simulate_keystrokes("escape ctrl-z");

    assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
    let variants = cx.read(|cx| {
        let document = workspace.read(cx).editor().read(cx).document();
        document.slots().first().map(|slot| slot.variants.len())
    });
    assert_eq!(variants, Some(2));
}

fn focused_on_editor(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    cx.update(|window, cx| editor.focus_handle(cx).is_focused(window))
}

#[gpui::test]
fn escape_closes_the_panel_and_the_editor_takes_the_keys_again(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a escape");

    assert!(panel(&workspace, cx).is_none());
    assert!(focused_on_editor(&workspace, cx));
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

#[gpui::test]
fn up_and_down_in_the_panel_show_each_variant_in_the_document(cx: &mut TestAppContext) {
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
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn provenances(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Provenance> {
    let panel = panel(workspace, cx).expect("the panel is open");
    cx.read(|cx| {
        panel
            .read(cx)
            .rows(cx)
            .into_iter()
            .map(|row| row.provenance)
            .collect()
    })
}

#[gpui::test]
fn return_on_a_variant_edits_it_and_an_edited_ai_variant_is_mixed(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up enter backspace backspace backspace backspace");
    cx.simulate_input("rty");
    cx.simulate_keystrokes("enter");

    assert_eq!(text(&workspace, cx), "Title\nThe party waits.");
    assert_eq!(
        provenances(&workspace, cx),
        vec![Provenance::Writer, Provenance::Writer, Provenance::Mixed]
    );
}

#[gpui::test]
fn escape_while_editing_drops_the_edit_and_keeps_the_panel_open(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up enter backspace escape");

    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
    assert!(panel(&workspace, cx).is_some());
}

#[gpui::test]
fn ctrl_backspace_deletes_the_selected_variant_and_the_original_shows(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up ctrl-backspace");

    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", true), row("client", false)]
    );
}

#[gpui::test]
fn the_original_can_be_deleted_and_the_next_variant_shows(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up up up ctrl-backspace");

    assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
    assert_eq!(
        rows(&workspace, cx),
        vec![row("client", true), row("patron", false)]
    );
}

#[gpui::test]
fn a_row_delete_plays_the_explosion_on_the_row_and_at_the_span(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up up ctrl-backspace");

    let panel = panel(&workspace, cx).expect("the panel is open");
    assert_eq!(cx.read(|cx| panel.read(cx).exploding_row()), Some(1));
    let effects = cx.read(|cx| workspace.read(cx).editor().read(cx).effects());
    assert_eq!(
        effects,
        vec![app::Effect {
            kind: app::EffectKind::Explosion,
            range: 10..16
        }]
    );
}

#[gpui::test]
fn deleting_all_but_one_variant_ends_the_slot_and_the_panel_keeps_its_text(
    cx: &mut TestAppContext,
) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up ctrl-backspace up ctrl-backspace");
    assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);
    let slots = cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .slots()
            .len()
    });
    assert_eq!(slots, 0);

    cx.simulate_input("buyer");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", false), row("buyer", true)]
    );
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

#[gpui::test]
fn a_click_on_a_slot_while_the_panel_is_open_shows_its_variants(cx: &mut TestAppContext) {
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
}

fn bright(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Range<usize>> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).bright_paragraph())
}

#[gpui::test]
fn while_the_panel_is_open_only_the_paragraph_of_its_slot_stays_bright(cx: &mut TestAppContext) {
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
}

#[gpui::test]
fn the_text_column_does_not_move_when_the_panel_opens(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    let before = point_at(&workspace, cx, 13);
    cx.simulate_keystrokes("ctrl-shift-a");

    assert_eq!(point_at(&workspace, cx, 13), before);
}

fn right_click(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, offset: usize) {
    let position = point_at(workspace, cx, offset);
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::none());
}

#[gpui::test]
fn the_context_menu_alternatives_row_opens_the_panel_on_the_word(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    right_click(&workspace, cx, 13);
    let row_bounds = cx
        .debug_bounds("context-menu-row-0")
        .expect("the menu shows the Alternatives row");
    cx.simulate_click(row_bounds.center(), Modifiers::none());

    assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);
}

#[gpui::test]
fn a_click_on_a_variant_row_shows_that_variant(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a");
    let row_bounds = cx
        .debug_bounds("alternatives-row-2")
        .expect("the panel shows the third Variant");
    cx.simulate_click(row_bounds.center(), Modifiers::none());

    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
}

#[gpui::test]
fn a_click_on_the_sentence_tab_shows_the_sentence_around_the_word(cx: &mut TestAppContext) {
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
}

#[gpui::test]
fn one_undo_in_the_editor_removes_a_variant_the_panel_added(cx: &mut TestAppContext) {
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
}
