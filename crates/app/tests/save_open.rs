mod system_text;

use std::fs;
use std::path::Path;

use app::{Notice, Workspace};
use gpui::{Entity, Keystroke, TestAppContext, VisualTestContext};
use hamish_core::{Document, Dropped, Level, Provenance};
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

/// Saves through Ctrl+S and the Save As dialog to `path`.
fn save_new(cx: &mut VisualTestContext, path: &Path) {
    cx.simulate_keystrokes("ctrl-s");
    let path = path.to_owned();
    cx.simulate_new_path_selection(|_| Some(path));
    cx.run_until_parked();
}

#[test]
fn first_save_of_a_new_document_asks_for_a_path_and_writes_one_md_file() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("draft.md");
        let (_workspace, cx) = open(cx, Document::new("Title\nThe customer waits."));

        cx.simulate_keystrokes("ctrl-s");
        assert!(cx.did_prompt_for_new_path(), "Save As dialog is open");
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();

        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .starts_with("Title\nThe customer waits.\n\n<!-- hamish\n")
        );
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["draft.md"]);
    });
}

#[test]
fn an_edit_makes_the_document_unsaved_and_ctrl_s_saves_it_to_the_same_path() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("draft.md");
        let (workspace, cx) = open(cx, Document::new("Title"));
        save_new(cx, &path);
        assert!(!unsaved(&workspace, cx));

        cx.simulate_keystrokes("end !");
        assert!(unsaved(&workspace, cx));

        cx.simulate_keystrokes("ctrl-s");
        cx.run_until_parked();

        assert!(
            !cx.did_prompt_for_new_path(),
            "no dialog after the first save"
        );
        assert_eq!(Document::open(&path).unwrap().document.text(), "Title!");
        assert!(!unsaved(&workspace, cx));
    });
}

#[test]
fn two_quick_saves_leave_the_last_text_on_disk() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("draft.md");
        let (workspace, cx) = open(cx, Document::new("Title"));
        save_new(cx, &path);

        cx.simulate_keystrokes("end ! ctrl-s ? ctrl-s");
        cx.run_until_parked();

        assert_eq!(Document::open(&path).unwrap().document.text(), "Title!?");
        assert!(!unsaved(&workspace, cx));
    });
}

#[test]
fn save_as_asks_for_a_path_every_time_and_moves_the_document_there() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let first = dir.path().join("first.md");
        let second = dir.path().join("second.md");
        let (workspace, cx) = open(cx, Document::new("Title"));
        save_new(cx, &first);

        cx.simulate_keystrokes("ctrl-shift-s");
        assert!(cx.did_prompt_for_new_path(), "Save As dialog is open");
        cx.simulate_new_path_selection(|_| Some(second.clone()));
        cx.run_until_parked();

        assert_eq!(Document::open(&second).unwrap().document.text(), "Title");
        let saved_at = cx.read(|cx| workspace.read(cx).document_path().map(Path::to_owned));
        assert_eq!(saved_at, Some(second));
    });
}

#[test]
fn a_cancelled_save_dialog_writes_nothing_and_stays_unsaved() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Document::new("Title"));
        cx.simulate_keystrokes("end !");

        cx.simulate_keystrokes("ctrl-s");
        cx.simulate_new_path_selection(|_| None);
        cx.run_until_parked();

        assert!(unsaved(&workspace, cx));
        assert_eq!(
            cx.read(|cx| workspace.read(cx).document_path().map(Path::to_owned)),
            None
        );
    });
}

#[test]
fn a_failed_save_shows_the_error_and_stays_unsaved() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("missing").join("draft.md");
        let (workspace, cx) = open(cx, Document::new("Title"));
        cx.simulate_keystrokes("end !");

        save_new(cx, &missing);

        let expected = format!(
            "{}.saving: No such file or directory (os error 2)",
            missing.display()
        );
        assert_eq!(
            cx.read(|cx| workspace.read(cx).notice().cloned()),
            Some(Notice::SaveFailed(expected))
        );
        assert!(unsaved(&workspace, cx));
    });
}

fn open_path(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, path: &Path) {
    let path = path.to_owned();
    workspace.update(cx, |workspace, cx| workspace.open_path(path, cx));
    cx.run_until_parked();
}

#[test]
fn open_shows_the_saved_document_with_its_slots() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("draft.md");
        let mut saved = Document::new("The quick fox.");
        let slot = saved
            .create_slot(4..9, Level::Word, "swift", Provenance::Writer)
            .unwrap();
        saved.set_active(slot, 1).unwrap();
        saved.save(&path).unwrap();
        let (workspace, cx) = open(cx, Document::new("Other"));

        open_path(&workspace, cx, &path);

        assert_eq!(text(&workspace, cx), "The swift fox.");
        let slots = cx.read(|cx| {
            workspace
                .read(cx)
                .editor()
                .read(cx)
                .document()
                .slots()
                .len()
        });
        assert_eq!(slots, 1);
        assert!(!unsaved(&workspace, cx));
        assert_eq!(cx.read(|cx| workspace.read(cx).notice().cloned()), None);
        let opened_at = cx.read(|cx| workspace.read(cx).document_path().map(Path::to_owned));
        assert_eq!(opened_at, Some(path));
    });
}

#[test]
fn open_after_an_outside_edit_shows_the_dropped_slot() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("draft.md");
        let mut saved = Document::new("The quick fox.");
        saved
            .create_slot(4..9, Level::Word, "swift", Provenance::Writer)
            .unwrap();
        saved.save(&path).unwrap();
        let file = fs::read_to_string(&path).unwrap();
        fs::write(&path, file.replace("The quick fox.", "The slow fox.")).unwrap();
        let (workspace, cx) = open(cx, Document::default());

        open_path(&workspace, cx, &path);

        assert_eq!(text(&workspace, cx), "The slow fox.");
        assert_eq!(
            cx.read(|cx| workspace.read(cx).notice().cloned()),
            Some(Notice::Dropped(vec![Dropped::Slot {
                level: Level::Word,
                text: "quick".to_owned(),
            }]))
        );
    });
}

#[test]
fn a_failed_open_shows_the_error_and_keeps_the_document() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("missing.md");
        let (workspace, cx) = open(cx, Document::new("Kept"));

        open_path(&workspace, cx, &missing);

        assert_eq!(text(&workspace, cx), "Kept");
        let expected = format!(
            "{}: No such file or directory (os error 2)",
            missing.display()
        );
        assert_eq!(
            cx.read(|cx| workspace.read(cx).notice().cloned()),
            Some(Notice::OpenFailed(expected))
        );
    });
}

#[test]
fn ctrl_o_with_unsaved_changes_asks_first_and_cancel_keeps_the_document() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Document::new("Title"));
        cx.simulate_keystrokes("end !");

        cx.simulate_keystrokes("ctrl-o");
        assert!(cx.has_pending_prompt(), "discard question is open");
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();

        assert_eq!(text(&workspace, cx), "Title!");
        assert!(unsaved(&workspace, cx));
    });
}

#[expect(clippy::unwrap_used, reason = "test fixture")]
fn press_without_waiting(cx: &mut VisualTestContext, keystroke: &'static str) {
    let keystroke = Keystroke::parse(keystroke).unwrap();
    cx.update(|window, cx| window.dispatch_keystroke(keystroke, cx));
}

#[test]
fn a_save_that_ends_after_ctrl_n_leaves_the_new_document_untitled() {
    system_text::run(50, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("draft.md");
        let (workspace, cx) = open(cx, Document::new("Title"));
        save_new(cx, &path);

        press_without_waiting(cx, "ctrl-s");
        press_without_waiting(cx, "ctrl-n");
        cx.run_until_parked();

        assert_eq!(text(&workspace, cx), "");
        assert_eq!(
            cx.read(|cx| workspace.read(cx).document_path().map(Path::to_owned)),
            None
        );
        cx.simulate_keystrokes("x ctrl-s");
        assert!(cx.did_prompt_for_new_path(), "Save As dialog is open");
    });
}

#[test]
fn a_save_that_ends_after_an_open_keeps_the_opened_path() {
    system_text::run(50, |cx| {
        let dir = TempDir::new().unwrap();
        let draft = dir.path().join("draft.md");
        let other = dir.path().join("other.md");
        Document::new("Other").save(&other).unwrap();
        let (workspace, cx) = open(cx, Document::new("Title"));
        save_new(cx, &draft);
        cx.simulate_keystrokes("end !");

        press_without_waiting(cx, "ctrl-s");
        let opened = other.clone();
        cx.update(|_, cx| workspace.update(cx, |workspace, cx| workspace.open_path(opened, cx)));
        cx.run_until_parked();

        assert_eq!(text(&workspace, cx), "Other");
        assert_eq!(
            cx.read(|cx| workspace.read(cx).document_path().map(Path::to_owned)),
            Some(other.clone())
        );
        assert!(!unsaved(&workspace, cx));
        cx.simulate_keystrokes("end ? ctrl-s");
        cx.run_until_parked();
        assert_eq!(Document::open(&draft).unwrap().document.text(), "Title!");
        assert_eq!(Document::open(&other).unwrap().document.text(), "Other?");
    });
}
