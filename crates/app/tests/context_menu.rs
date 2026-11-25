use app::{ContextMenuItem, Workspace};
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, MouseButton, Point, TestAppContext,
    VisualTestContext, point, px,
};
use ka_teb_core::Document;

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
