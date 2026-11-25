use app::{ContextMenuItem, Workspace};
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, MouseButton, Point, TestAppContext,
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

fn right_click(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, offset: usize) {
    let position = point_at(workspace, cx, offset);
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::none());
}

fn menu(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<Vec<(String, String)>> {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .context_menu_items()
            .map(|items| {
                items
                    .into_iter()
                    .map(|ContextMenuItem { label, shortcut }| (label, shortcut.to_owned()))
                    .collect()
            })
    })
}

fn items(rows: [(&str, &str); 4]) -> Vec<(String, String)> {
    rows.into_iter()
        .map(|(label, shortcut)| (label.to_owned(), shortcut.to_owned()))
        .collect()
}

/// "Title\nThe customer waits." ("customer" is 10..18).
fn customer_document() -> Document {
    Document::new("Title\nThe customer waits.")
}

#[gpui::test]
fn right_click_on_a_word_names_the_word_in_the_menu(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    right_click(&workspace, cx, 13);

    assert_eq!(
        menu(&workspace, cx),
        Some(items([
            ("Alternatives for \"customer\"", "Ctrl+Shift+A"),
            ("AI alternatives for \"customer\"", "Ctrl+Shift+G"),
            ("Ghost it", "Ctrl+/"),
            ("Stash this in Overflow", "Ctrl+Shift+X"),
        ]))
    );
}

fn ghosts(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Vec<std::ops::Range<usize>> {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .ghost_ranges()
    })
}

fn counter(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| workspace.read(cx).editor().read(cx).counter_label())
}

#[gpui::test]
fn right_click_on_a_selection_acts_on_the_selection(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes("down shift-end");
    right_click(&workspace, cx, 13);

    assert_eq!(
        menu(&workspace, cx),
        Some(items([
            ("Alternatives for selection", "Ctrl+Shift+A"),
            ("AI alternatives for selection", "Ctrl+Shift+G"),
            ("Ghost it", "Ctrl+/"),
            ("Stash this in Overflow", "Ctrl+Shift+X"),
        ]))
    );
}

#[gpui::test]
fn right_click_on_a_space_shows_no_menu(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nThe  customer"));

    right_click(&workspace, cx, 10);

    assert_eq!(menu(&workspace, cx), None);
}

#[gpui::test]
fn ctrl_slash_ghosts_the_selection_and_undo_brings_it_back(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes("down shift-end ctrl-/");
    assert_eq!(ghosts(&workspace, cx), vec![6..25]);
    assert_eq!(counter(&workspace, cx), "0 words 0 chars selected");

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(ghosts(&workspace, cx), Vec::<std::ops::Range<usize>>::new());
}

#[gpui::test]
fn ctrl_slash_with_no_selection_ghosts_the_word_at_the_caret(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes("down right right right right right ctrl-/");

    assert_eq!(ghosts(&workspace, cx), vec![10..18]);
}

#[gpui::test]
fn ctrl_slash_inside_a_ghost_range_revives_it(cx: &mut TestAppContext) {
    let mut document = customer_document();
    assert!(document.ghost(6..25).is_ok(), "valid range");
    let (workspace, cx) = open(cx, document);

    cx.simulate_keystrokes("down right right right right right ctrl-/");

    assert_eq!(ghosts(&workspace, cx), Vec::<std::ops::Range<usize>>::new());
}

#[gpui::test]
fn right_click_inside_a_ghost_range_offers_revive_it(cx: &mut TestAppContext) {
    let mut document = customer_document();
    assert!(document.ghost(10..18).is_ok(), "valid range");
    let (workspace, cx) = open(cx, document);

    right_click(&workspace, cx, 13);

    let labels = menu(&workspace, cx)
        .map(|rows| rows.into_iter().map(|(label, _)| label).collect::<Vec<_>>());
    assert_eq!(
        labels,
        Some(vec![
            "Alternatives for \"customer\"".to_owned(),
            "AI alternatives for \"customer\"".to_owned(),
            "Revive it".to_owned(),
            "Stash this in Overflow".to_owned(),
        ])
    );
}

#[gpui::test]
fn a_click_on_ghost_it_ghosts_the_word_and_closes_the_menu(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    right_click(&workspace, cx, 13);
    let row = cx
        .debug_bounds("context-menu-row-2")
        .expect("the menu shows the Ghost it row");
    cx.simulate_click(row.center(), Modifiers::none());

    assert_eq!(ghosts(&workspace, cx), vec![10..18]);
    assert_eq!(menu(&workspace, cx), None);
}

#[gpui::test]
fn escape_closes_the_menu(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    right_click(&workspace, cx, 13);
    cx.simulate_keystrokes("escape");

    assert_eq!(menu(&workspace, cx), None);
}

#[gpui::test]
fn a_click_outside_the_menu_closes_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    right_click(&workspace, cx, 13);
    let elsewhere = point_at(&workspace, cx, 2);
    cx.simulate_click(elsewhere, Modifiers::none());

    assert_eq!(menu(&workspace, cx), None);
}

#[gpui::test]
fn a_menu_action_not_built_yet_closes_the_menu_and_keeps_the_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    right_click(&workspace, cx, 13);
    cx.simulate_keystrokes("ctrl-shift-g");

    assert_eq!(menu(&workspace, cx), None);
    let text = cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .text()
            .to_owned()
    });
    assert_eq!(text, "Title\nThe customer waits.");
}
