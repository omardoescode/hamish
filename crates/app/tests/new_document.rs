mod system_text;

use std::path::Path;

use app::{OpenAlternatives, ToggleLab, ToggleOverflow, Workspace};
use gpui::{Entity, TestAppContext, VisualTestContext};
use hamish_core::{Document, Level, Provenance};
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

fn unsaved(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).editor().read(cx).is_unsaved())
}

fn path(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<std::path::PathBuf> {
    cx.read(|cx| workspace.read(cx).document_path().map(Path::to_owned))
}

#[expect(clippy::unwrap_used, reason = "test fixture")]
fn saved_document() -> Document {
    let mut document = Document::new("The quick fox.");
    document
        .create_slot(4..9, Level::Word, "swift", Provenance::Writer)
        .unwrap();
    document.edit_overflow(0..0, "a note").unwrap();
    document
}

#[test]
fn ctrl_n_on_a_saved_document_starts_an_empty_untitled_document() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("draft.md");
        saved_document().save(&file).unwrap();
        let (workspace, cx) = open(cx, Document::default());
        workspace.update(cx, |workspace, cx| workspace.open_path(file, cx));
        cx.run_until_parked();
        cx.simulate_keystrokes("end ! ctrl-s");
        cx.run_until_parked();
        assert!(!unsaved(&workspace, cx));

        cx.simulate_keystrokes("ctrl-n");
        cx.run_until_parked();

        assert!(!cx.has_pending_prompt(), "a saved Document asks nothing");
        assert_eq!(text(&workspace, cx), "");
        let (slots, overflow) = cx.read(|cx| {
            let editor = workspace.read(cx).editor().read(cx);
            (
                editor.document().slots().len(),
                editor.document().overflow().text().to_owned(),
            )
        });
        assert_eq!(slots, 0);
        assert_eq!(overflow, "");
        assert_eq!(path(&workspace, cx), None);
        assert!(!unsaved(&workspace, cx));
        assert_eq!(cx.read(|cx| workspace.read(cx).title_label(cx)), "Untitled");
    });
}

#[test]
fn the_new_document_has_no_undo_steps_and_its_first_save_asks_for_a_path() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("draft.md");
        let (workspace, cx) = open(cx, Document::new("Title"));
        cx.simulate_keystrokes("ctrl-s");
        cx.simulate_new_path_selection(|_| Some(file.clone()));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-n ctrl-z");
        cx.run_until_parked();
        assert_eq!(text(&workspace, cx), "");
        assert!(!unsaved(&workspace, cx));

        cx.simulate_keystrokes("x ctrl-s");
        assert!(cx.did_prompt_for_new_path(), "Save As dialog is open");
    });
}

#[test]
fn ctrl_n_with_unsaved_changes_asks_first_and_cancel_keeps_the_document() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Document::new("Title"));
        cx.simulate_keystrokes("end !");

        cx.simulate_keystrokes("ctrl-n");
        assert!(cx.has_pending_prompt(), "discard question is open");
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();

        assert_eq!(text(&workspace, cx), "Title!");
        assert!(unsaved(&workspace, cx));
    });
}

#[test]
fn ctrl_n_with_unsaved_changes_and_discard_starts_an_empty_document() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Document::new("Title"));
        cx.simulate_keystrokes("end !");

        cx.simulate_keystrokes("ctrl-n");
        cx.simulate_prompt_answer("Discard");
        cx.run_until_parked();

        assert_eq!(text(&workspace, cx), "");
        assert!(!unsaved(&workspace, cx));
    });
}

#[test]
fn ctrl_n_closes_the_panels_as_on_app_start() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, saved_document());
        cx.dispatch_action(ToggleLab);
        cx.dispatch_action(OpenAlternatives);
        cx.dispatch_action(ToggleOverflow);
        cx.run_until_parked();
        let (lab, alternatives, overflow) = panels(&workspace, cx);
        assert!(lab && alternatives && overflow, "all panels are open");

        cx.dispatch_action(app::NewDocument);
        cx.run_until_parked();

        assert_eq!(panels(&workspace, cx), (false, false, false));
        cx.simulate_keystrokes("a");
        assert_eq!(text(&workspace, cx), "a", "the Document has the keyboard");
    });
}

fn panels(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (bool, bool, bool) {
    cx.read(|cx| {
        let workspace = workspace.read(cx);
        (
            workspace.lab().read(cx).is_open(),
            workspace.alternatives_panel().is_some(),
            workspace.overflow_open(),
        )
    })
}
