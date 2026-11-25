use app::Workspace;
use gpui::{Entity, TestAppContext, VisualTestContext};
use hamish_core::Document;
use tempfile::TempDir;

fn open(
    cx: &mut TestAppContext,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn shown_title(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| workspace.read(cx).title_label(cx))
}

fn window_title(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.window_title()
}

#[gpui::test]
fn a_new_document_shows_its_first_line_as_the_title(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Harbour\nThe boats wait."));

    assert_eq!(shown_title(&workspace, cx), "Harbour");
    assert_eq!(window_title(cx).as_deref(), Some("Harbour"));
}

#[gpui::test]
fn an_empty_document_is_untitled(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::default());

    assert_eq!(shown_title(&workspace, cx), "Untitled");
    assert_eq!(window_title(cx).as_deref(), Some("Untitled"));
}

#[gpui::test]
fn an_edit_adds_the_unsaved_dot_and_a_save_shows_the_file_name(cx: &mut TestAppContext) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("letter.md");
    let (workspace, cx) = open(cx, Document::new("Harbour"));

    cx.simulate_input("s");
    assert_eq!(shown_title(&workspace, cx), "sHarbour •");
    assert_eq!(window_title(cx).as_deref(), Some("sHarbour •"));

    cx.simulate_keystrokes("ctrl-s");
    cx.simulate_new_path_selection(|_| Some(path));
    cx.run_until_parked();

    assert_eq!(shown_title(&workspace, cx), "letter");
    assert_eq!(window_title(cx).as_deref(), Some("letter"));
}
