mod system_text;

use std::fs;

use app::{Notice, Workspace};
use gpui::{Entity, TestAppContext, VisualTestContext};
use hamish_core::{Document, LineError, LoadedSettings, Settings};
use tempfile::TempDir;

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new(text), window, cx));
    cx.update(|_, cx| {
        app::use_settings(
            None,
            LoadedSettings {
                settings: Settings {
                    vim_mode: true,
                    ..Settings::default()
                },
                problem: None,
            },
            cx,
        );
    });
    cx.run_until_parked();
    (workspace, cx)
}

/// Types `line` char by char on the Command line.
fn type_line(cx: &mut VisualTestContext, line: &str) {
    let keys: Vec<String> = line
        .chars()
        .map(|typed| match typed {
            ' ' => "space".to_owned(),
            typed => typed.to_string(),
        })
        .collect();
    cx.simulate_keystrokes(&keys.join(" "));
    cx.run_until_parked();
}

/// `:`, then `line`, then Enter.
fn run_line(cx: &mut VisualTestContext, line: &str) {
    cx.simulate_keystrokes(":");
    type_line(cx, line);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

fn line_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.read(|cx| workspace.read(cx).command_line().map(str::to_owned))
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

fn notice(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Notice> {
    cx.read(|cx| workspace.read(cx).notice().cloned())
}

#[test]
fn colon_opens_the_command_line_at_the_window_bottom_and_escape_closes_it() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two");

        cx.simulate_keystrokes(":");
        type_line(cx, "wq");

        assert_eq!(line_text(&workspace, cx).as_deref(), Some("wq"));
        let line = cx
            .debug_bounds("command-line")
            .expect("the Command line shows");
        let window = cx.update(|window, _| window.viewport_size());
        assert_eq!(line.bottom(), window.height);
        assert_eq!(text(&workspace, cx), "one two");

        cx.simulate_keystrokes("escape");
        assert_eq!(line_text(&workspace, cx), None);
        assert!(cx.debug_bounds("command-line").is_none());
        cx.simulate_keystrokes("x");
        assert_eq!(text(&workspace, cx), "ne two");
    });
}

#[test]
fn w_with_a_path_saves_there_on_the_background_executor() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("draft.md");
        let (workspace, cx) = open(cx, "The draft.");

        run_line(cx, &format!("w {}", path.display()));

        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .starts_with("The draft.\n")
        );
        let saved_to = cx.read(|cx| workspace.read(cx).document_path().map(ToOwned::to_owned));
        assert_eq!(saved_to, Some(path));
        assert_eq!(line_text(&workspace, cx), None);
    });
}

#[test]
fn wq_with_a_path_saves_there() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("done.md");
        let (_workspace, cx) = open(cx, "Done.");

        run_line(cx, &format!("wq {}", path.display()));

        assert!(fs::read_to_string(&path).unwrap().starts_with("Done.\n"));
    });
}

#[test]
fn q_with_unsaved_changes_refuses_with_a_notice() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one");
        cx.simulate_keystrokes("x");

        run_line(cx, "q");

        assert_eq!(
            notice(&workspace, cx),
            Some(Notice::CommandFailed(LineError::Unsaved))
        );
    });
}

#[test]
fn an_unknown_command_shows_a_notice() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one");

        run_line(cx, "source rc");

        assert_eq!(
            notice(&workspace, cx),
            Some(Notice::CommandFailed(LineError::Unknown(
                "source".to_owned()
            )))
        );
    });
}

#[test]
fn e_opens_a_file_and_refuses_with_unsaved_changes_unless_forced() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("other.md");
        fs::write(&path, "Other text.").unwrap();
        let (workspace, cx) = open(cx, "one");
        cx.simulate_keystrokes("x");

        run_line(cx, &format!("e {}", path.display()));
        assert_eq!(text(&workspace, cx), "ne");
        assert_eq!(
            notice(&workspace, cx),
            Some(Notice::CommandFailed(LineError::Unsaved))
        );

        run_line(cx, &format!("e! {}", path.display()));
        assert_eq!(text(&workspace, cx), "Other text.");
    });
}

#[test]
fn set_novim_turns_vim_mode_off() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one");

        run_line(cx, "set novim");

        assert_eq!(cx.read(|cx| workspace.read(cx).vim_status(cx)), None);
        cx.simulate_keystrokes("x");
        assert_eq!(text(&workspace, cx), "xone");
    });
}

#[test]
fn chrome_turns_the_chrome_bar_off_and_on() {
    system_text::run(1, |cx| {
        let (_workspace, cx) = open(cx, "one");

        run_line(cx, "chrome");
        assert!(cx.debug_bounds("chrome-bar").is_none());

        run_line(cx, "chrome");
        assert!(cx.debug_bounds("chrome-bar").is_some());
    });
}

#[test]
fn settings_opens_settings() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one");

        run_line(cx, "settings");

        assert!(cx.read(|cx| workspace.read(cx).settings_open()));
    });
}

#[test]
fn tab_completes_a_path_from_the_directory_on_disk() {
    system_text::run(1, |cx| {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("draft.md"), "").unwrap();
        let (workspace, cx) = open(cx, "one");

        cx.simulate_keystrokes(":");
        type_line(cx, &format!("e {}/dr", dir.path().display()));
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();

        assert_eq!(
            line_text(&workspace, cx),
            Some(format!("e {}/draft.md", dir.path().display()))
        );
    });
}

#[test]
fn up_shows_the_line_run_before() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one");
        run_line(cx, "chrome");

        cx.simulate_keystrokes(": up");

        assert_eq!(line_text(&workspace, cx).as_deref(), Some("chrome"));
    });
}

#[test]
fn the_caret_moves_and_ctrl_keys_edit_before_it() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one");

        cx.simulate_keystrokes(":");
        type_line(cx, "e one two");
        assert!(cx.debug_bounds("command-line-caret").is_some());
        cx.simulate_keystrokes("ctrl-w");
        assert_eq!(line_text(&workspace, cx).as_deref(), Some("e one "));
        cx.simulate_keystrokes("ctrl-b x");
        assert_eq!(line_text(&workspace, cx).as_deref(), Some("xe one "));
        cx.simulate_keystrokes("ctrl-e y left left backspace");
        assert_eq!(line_text(&workspace, cx).as_deref(), Some("xe on y"));
        cx.simulate_keystrokes("right ctrl-u");
        assert_eq!(line_text(&workspace, cx).as_deref(), Some("y"));
        cx.simulate_keystrokes("home z end");
        assert_eq!(line_text(&workspace, cx).as_deref(), Some("zy"));
    });
}
