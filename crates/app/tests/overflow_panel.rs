use app::Workspace;
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};
use hamish_core::Document;

fn open(
    cx: &mut TestAppContext,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn overflow_open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).overflow_open())
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

fn overflow(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .overflow()
            .text()
            .to_owned()
    })
}

#[expect(clippy::expect_used, reason = "test fixture")]
/// A window point in the middle of the Document row of byte `offset`, just right of it.
fn document_point(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    offset: usize,
) -> Point<Pixels> {
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

#[expect(clippy::expect_used, reason = "test fixture")]
/// A window point in the middle of the Overflow row of byte `offset`, just right of it.
fn overflow_point(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    offset: usize,
) -> Point<Pixels> {
    let panel = cx.read(|cx| workspace.read(cx).overflow_panel().clone());
    let bounds = cx
        .update(|window, cx| {
            panel.update(cx, |panel, cx| {
                panel.bounds_for_range(offset..offset, Bounds::default(), window, cx)
            })
        })
        .expect("the panel painted the Overflow text");
    bounds.origin + point(px(3.), bounds.size.height / 2.)
}

/// "Title\nBody." with Overflow "first block\n\nsecond block".
fn document_with_overflow() -> Document {
    let mut document = Document::new("Title\nBody.");
    assert!(
        document
            .edit_overflow(0..0, "first block\n\nsecond block")
            .is_ok(),
        "valid range"
    );
    document
}

#[gpui::test]
fn ctrl_shift_o_opens_and_closes_overflow(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nBody."));
    assert!(!overflow_open(&workspace, cx));

    cx.simulate_keystrokes("ctrl-shift-o");
    assert!(overflow_open(&workspace, cx));

    cx.simulate_keystrokes("ctrl-shift-o");
    assert!(!overflow_open(&workspace, cx));
}

#[gpui::test]
fn the_overflow_button_in_the_chrome_opens_and_closes_overflow(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nBody."));
    cx.simulate_click(point(px(30.), px(28.)), Modifiers::none());

    let button = cx
        .debug_bounds("overflow-button")
        .expect("the chrome shows the Overflow button");
    cx.simulate_click(button.center(), Modifiers::none());
    assert!(overflow_open(&workspace, cx));

    cx.simulate_click(button.center(), Modifiers::none());
    assert!(!overflow_open(&workspace, cx));
}

#[gpui::test]
fn typing_in_overflow_edits_overflow_and_not_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nBody."));

    cx.simulate_keystrokes("ctrl-shift-o");
    cx.simulate_input("a note");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("link");
    cx.simulate_keystrokes("backspace left left shift-left");
    cx.simulate_input("o");

    assert_eq!(overflow(&workspace, cx), "a note\noin");
    assert_eq!(text(&workspace, cx), "Title\nBody.");
}

#[gpui::test]
fn undo_in_overflow_reverts_typing_as_one_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nBody."));

    cx.simulate_keystrokes("ctrl-shift-o");
    cx.simulate_input("a note");
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(overflow(&workspace, cx), "");

    cx.simulate_keystrokes("ctrl-shift-z");
    assert_eq!(overflow(&workspace, cx), "a note");
}

#[gpui::test]
fn escape_in_overflow_gives_the_keyboard_back_to_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nBody."));

    cx.simulate_keystrokes("ctrl-shift-o escape");
    cx.simulate_input("A ");

    assert_eq!(text(&workspace, cx), "A Title\nBody.");
    assert_eq!(overflow(&workspace, cx), "");
    assert!(overflow_open(&workspace, cx));
}

#[gpui::test]
fn ctrl_shift_x_stashes_the_selection_and_shows_overflow(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nCut this."));

    cx.simulate_keystrokes("down shift-end ctrl-shift-x");

    assert_eq!(text(&workspace, cx), "Title\n");
    assert_eq!(overflow(&workspace, cx), "Cut this.");
    assert!(overflow_open(&workspace, cx));
}

#[gpui::test]
fn undo_brings_stashed_text_back_from_overflow(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nCut this."));

    cx.simulate_keystrokes("down shift-end ctrl-shift-x ctrl-z");

    assert_eq!(text(&workspace, cx), "Title\nCut this.");
    assert_eq!(overflow(&workspace, cx), "");
}

#[gpui::test]
fn the_stash_menu_row_moves_the_word_under_the_pointer(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nThe customer waits."));

    let position = document_point(&workspace, cx, 13);
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::none());
    let row = cx
        .debug_bounds("context-menu-row-3")
        .expect("the menu shows the Stash row");
    cx.simulate_click(row.center(), Modifiers::none());

    assert_eq!(text(&workspace, cx), "Title\nThe  waits.");
    assert_eq!(overflow(&workspace, cx), "customer");
}

#[gpui::test]
fn ctrl_return_puts_the_block_at_the_caret_into_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, document_with_overflow());

    cx.simulate_keystrokes("down end ctrl-shift-o ctrl-a right ctrl-enter");

    assert_eq!(text(&workspace, cx), "Title\nBody.second block");
    assert_eq!(overflow(&workspace, cx), "first block\n\nsecond block");
}

#[gpui::test]
fn undo_takes_a_used_block_back_out_of_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, document_with_overflow());

    cx.simulate_keystrokes("ctrl-shift-o ctrl-enter ctrl-z");

    assert_eq!(text(&workspace, cx), "Title\nBody.");
}

#[gpui::test]
fn a_block_dragged_into_the_page_goes_in_at_the_drop_point(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, document_with_overflow());
    cx.simulate_keystrokes("ctrl-shift-o");

    let from = overflow_point(&workspace, cx, 3);
    let to = document_point(&workspace, cx, 6);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());

    assert_eq!(text(&workspace, cx), "Title\nfirst block Body.");
    assert_eq!(overflow(&workspace, cx), "first block\n\nsecond block");
}

#[gpui::test]
fn a_block_dragged_over_the_page_shows_a_drop_caret_and_dims_other_paragraphs(
    cx: &mut TestAppContext,
) {
    let (workspace, cx) = open(cx, document_with_overflow());
    cx.simulate_keystrokes("ctrl-shift-o");
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());

    let from = overflow_point(&workspace, cx, 3);
    let to = document_point(&workspace, cx, 8);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    // The first move starts the drag; the next one is a move of the drag.
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());

    assert_eq!(cx.read(|cx| editor.read(cx).drop_caret()), Some(8));
    assert_eq!(
        cx.read(|cx| editor.read(cx).bright_paragraph()),
        Some(6..11)
    );

    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());

    assert_eq!(cx.read(|cx| editor.read(cx).drop_caret()), None);
    assert_eq!(cx.read(|cx| editor.read(cx).bright_paragraph()), None);
}

#[gpui::test]
fn a_drag_that_ends_off_the_page_leaves_no_drop_caret(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, document_with_overflow());
    cx.simulate_keystrokes("ctrl-shift-o");
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());

    let from = overflow_point(&workspace, cx, 3);
    let to = document_point(&workspace, cx, 8);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(from, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(text(&workspace, cx), "Title\nBody.");
    assert_eq!(cx.read(|cx| editor.read(cx).drop_caret()), None);
}
