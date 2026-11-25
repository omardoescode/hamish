use app::Workspace;
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px};
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
fn backspace_deletes_the_char_before_the_cursor(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "");

    cx.simulate_input("Hello");
    cx.simulate_keystrokes("backspace");

    assert_eq!(text(&workspace, cx), "Hell");
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
fn a_cursor_move_starts_a_new_undo_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "");

    cx.simulate_input("ab");
    cx.simulate_keystrokes("left");
    cx.simulate_input("c");
    assert_eq!(text(&workspace, cx), "acb");

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&workspace, cx), "ab");
}

#[gpui::test]
fn consecutive_backspaces_undo_as_one_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "Hello");

    cx.simulate_keystrokes("end backspace backspace backspace");
    assert_eq!(text(&workspace, cx), "He");

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&workspace, cx), "Hello");
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
fn typing_replaces_the_selection(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "Hello world");

    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("Hi");

    assert_eq!(text(&workspace, cx), "Hi");
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
