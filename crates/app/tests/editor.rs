use std::ops::Range;

use app::Workspace;
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    Pixels, Point, TestAppContext, VisualTestContext, point, px,
};
use ka_teb_core::Document;

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let document = Document::new(text);
    cx.add_window_view(|window, cx| Workspace::new(document, window, cx))
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

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range<usize> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).selection())
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

fn counter(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| workspace.read(cx).editor().read(cx).counter_label())
}

fn chrome_visible(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).chrome_visible())
}

#[gpui::test]
fn typing_inserts_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "");

    cx.simulate_input("Hello world");

    assert_eq!(text(&workspace, cx), "Hello world");
}

#[gpui::test]
fn undo_reverts_consecutive_typing_as_one_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "");

    cx.simulate_input("Hello world");
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&workspace, cx), "");

    cx.simulate_keystrokes("ctrl-shift-z");
    assert_eq!(text(&workspace, cx), "Hello world");
}

#[gpui::test]
fn counter_shows_document_counts(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "Hello dear world");

    assert_eq!(counter(&workspace, cx), "3 words 16 chars");

    cx.simulate_input("Oh, ");
    assert_eq!(counter(&workspace, cx), "4 words 20 chars");
}

#[gpui::test]
fn counter_shows_selection_counts(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "Title\nHello world");

    cx.simulate_keystrokes("down shift-end");

    assert_eq!(counter(&workspace, cx), "2 words 11 chars selected");
}

#[gpui::test]
fn enter_splits_the_line_and_down_moves_to_the_next_line(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "abcd");

    cx.simulate_keystrokes("right right enter");
    assert_eq!(text(&workspace, cx), "ab\ncd");

    cx.simulate_keystrokes("up down right");
    cx.simulate_input("X");
    assert_eq!(text(&workspace, cx), "ab\ncXd");
}

#[gpui::test]
fn mouse_drag_selects_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "Hello world");

    cx.simulate_mouse_down(
        point(px(600.), px(5.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(
        point(px(600.), px(600.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(px(600.), px(600.)),
        MouseButton::Left,
        Modifiers::none(),
    );

    assert_eq!(counter(&workspace, cx), "2 words 11 chars selected");
}

#[gpui::test]
fn a_click_on_the_counter_toggles_the_chrome(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "Hello");
    assert!(!chrome_visible(&workspace, cx));

    cx.simulate_click(point(px(30.), px(28.)), Modifiers::none());
    assert!(chrome_visible(&workspace, cx));

    cx.simulate_click(point(px(30.), px(28.)), Modifiers::none());
    assert!(!chrome_visible(&workspace, cx));
}

#[gpui::test]
fn ctrl_left_and_right_move_the_caret_by_word_and_shift_selects(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "Title\nThe red cat.");

    cx.simulate_keystrokes("down ctrl-right ctrl-right");
    assert_eq!(selection(&workspace, cx), 13..13);

    cx.simulate_keystrokes("ctrl-left");
    assert_eq!(selection(&workspace, cx), 10..10);

    cx.simulate_keystrokes("ctrl-shift-right");
    assert_eq!(selection(&workspace, cx), 10..13);
}

#[gpui::test]
fn home_and_end_go_to_the_start_and_end_of_a_wrapped_paragraph(cx: &mut TestAppContext) {
    let paragraph = format!("{}end", "word ".repeat(80));
    let (workspace, cx) = open(cx, &format!("Title\n{paragraph}"));
    cx.run_until_parked();

    cx.simulate_keystrokes("down end");
    assert_eq!(selection(&workspace, cx), 409..409);

    cx.simulate_keystrokes("home");
    assert_eq!(selection(&workspace, cx), 6..6);

    cx.simulate_keystrokes("shift-end");
    assert_eq!(selection(&workspace, cx), 6..409);
}

#[gpui::test]
fn up_and_down_keep_the_column_over_a_shorter_line(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "T\nabcdefgh\nab\nabcdefgh");

    cx.simulate_keystrokes("down right right right right right right down");
    assert_eq!(selection(&workspace, cx), 13..13);

    cx.simulate_keystrokes("down");
    assert_eq!(selection(&workspace, cx), 20..20);

    cx.simulate_keystrokes("up up");
    assert_eq!(selection(&workspace, cx), 8..8);
}

#[gpui::test]
fn a_double_click_selects_the_word(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "Title\nThe red cat.");
    let at = document_point(&workspace, cx, 11);

    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
    });

    assert_eq!(selection(&workspace, cx), 10..13);
}
