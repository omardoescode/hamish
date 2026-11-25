use app::Workspace;
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, Point, TestAppContext, VisualTestContext, point,
    px,
};
use ka_teb_core::{Document, Level, Provenance, SlotId};

fn open(
    cx: &mut TestAppContext,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
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

fn hover(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, offset: usize) {
    let position = point_at(workspace, cx, offset);
    cx.simulate_mouse_move(position, None, Modifiers::none());
}

fn hovered_slot(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<SlotId> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).hovered_slot())
}

/// "Title\nIt is a tension here." with a Word Slot on "tension" (Variants "tension", "struggle").
#[expect(clippy::expect_used, reason = "test fixture")]
fn word_slot_document() -> (Document, SlotId) {
    let mut document = Document::new("Title\nIt is a tension here.");
    let slot = document
        .create_slot(14..21, Level::Word, "struggle", Provenance::Writer)
        .expect("valid range");
    (document, slot)
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

fn counter(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| workspace.read(cx).editor().read(cx).counter_label())
}

/// "Title\nIt is an eraser here." with a Word Slot on "eraser" (Variants "eraser", "thumbtack").
#[expect(clippy::expect_used, reason = "test fixture")]
fn article_document() -> Document {
    let mut document = Document::new("Title\nIt is an eraser here.");
    document
        .create_slot(15..21, Level::Word, "thumbtack", Provenance::Writer)
        .expect("valid range");
    document
}

#[gpui::test]
fn down_over_a_slot_shows_the_next_variant_with_article_agreement(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, article_document());

    hover(&workspace, cx, 17);
    cx.simulate_keystrokes("down");

    assert_eq!(text(&workspace, cx), "Title\nIt is a thumbtack here.");
    assert_eq!(counter(&workspace, cx), "6 words 29 chars");
}

#[gpui::test]
fn up_over_a_slot_shows_the_previous_variant_and_wraps(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, article_document());

    hover(&workspace, cx, 17);
    cx.simulate_keystrokes("up");
    assert_eq!(text(&workspace, cx), "Title\nIt is a thumbtack here.");

    cx.simulate_keystrokes("up");
    assert_eq!(text(&workspace, cx), "Title\nIt is an eraser here.");
}

#[gpui::test]
fn undo_reverts_a_variant_switch(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, article_document());

    hover(&workspace, cx, 17);
    cx.simulate_keystrokes("down ctrl-z");

    assert_eq!(text(&workspace, cx), "Title\nIt is an eraser here.");
}

#[gpui::test]
fn the_pointer_over_a_slot_hovers_it(cx: &mut TestAppContext) {
    let (document, slot) = word_slot_document();
    let (workspace, cx) = open(cx, document);

    hover(&workspace, cx, 16);
    assert_eq!(hovered_slot(&workspace, cx), Some(slot));

    hover(&workspace, cx, 7);
    assert_eq!(hovered_slot(&workspace, cx), None);
}
