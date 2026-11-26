mod system_text;

use std::fs;
use std::path::Path;

use app::{Notice, Workspace};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use hamish_core::{Document, LoadedSettings, Settings};
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test fixture")]
/// Opens `text` with Vim mode `vim_mode` and the hamishrc `rc` in a new config dir.
fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    rc: &str,
    vim_mode: bool,
) -> (Entity<Workspace>, &'a mut VisualTestContext, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hamishrc");
    fs::write(&path, rc).unwrap();
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    cx.update(|cx| {
        app::use_settings(
            None,
            LoadedSettings {
                settings: Settings {
                    vim_mode,
                    ..Settings::default()
                },
                problem: None,
            },
            cx,
        );
        app::use_hamishrc(Some(path), cx);
    });
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new(text), window, cx));
    cx.run_until_parked();
    (workspace, cx, dir)
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

fn status(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.read(|cx| workspace.read(cx).vim_status(cx))
}

fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

#[expect(clippy::unwrap_used, reason = "test fixture")]
fn rewrite(dir: &TempDir, rc: &str) {
    fs::write(Path::new(dir.path()).join("hamishrc"), rc).unwrap();
}

#[test]
fn a_normal_mode_map_of_the_hamishrc_runs_its_keys() {
    system_text::run(1, |cx| {
        let (workspace, cx, _dir) = open(cx, "one\ntwo", "nnoremap X dd\n", true);

        keys(cx, "shift-x");

        assert_eq!(text(&workspace, cx), "two");
    });
}

#[test]
fn with_vim_mode_off_the_hamishrc_maps_no_key() {
    system_text::run(1, |cx| {
        let (workspace, cx, _dir) = open(cx, "one", "nnoremap X dd\n", false);

        keys(cx, "shift-x");

        assert_eq!(text(&workspace, cx), "Xone");
    });
}

#[test]
fn an_insert_mode_map_holds_its_first_key_and_types_it_when_the_next_key_differs() {
    system_text::run(1, |cx| {
        let (workspace, cx, _dir) = open(cx, "", "inoremap jj <Esc>\n", true);

        keys(cx, "i j j");
        assert_eq!(text(&workspace, cx), "");
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));

        keys(cx, "i j a");
        assert_eq!(text(&workspace, cx), "ja");
    });
}

#[test]
fn escape_after_a_held_map_key_types_that_key_then_goes_to_normal_mode() {
    system_text::run(1, |cx| {
        let (workspace, cx, _dir) = open(cx, "", "inoremap jj <Esc>\n", true);

        keys(cx, "i j escape");

        assert_eq!(text(&workspace, cx), "j");
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
    });
}

#[test]
fn a_key_vim_mode_does_not_read_after_a_held_map_key_types_that_key_then_its_own_text() {
    system_text::run(1, |cx| {
        let (workspace, cx, _dir) = open(cx, "", "inoremap jj <Esc>\n", true);

        keys(cx, "i alt-a->x j alt-a->x");

        assert_eq!(text(&workspace, cx), "xjx");
    });
}

#[test]
fn a_map_to_an_action_name_runs_that_action() {
    system_text::run(1, |cx| {
        let (workspace, cx, _dir) = open(cx, "one", "nnoremap <leader>o :ToggleOverflow\n", true);

        keys(cx, "space o");

        assert!(cx.read(|cx| workspace.read(cx).overflow_open()));
    });
}

#[test]
fn a_bad_line_shows_its_line_number_in_a_notice_and_the_good_lines_load() {
    system_text::run(1, |cx| {
        let (workspace, cx, _dir) = open(
            cx,
            "one\ntwo",
            "\" maps\nnmap Q <Nope>\nnnoremap X dd\n",
            true,
        );

        assert_eq!(
            notice(&workspace, cx),
            Some(Notice::Hamishrc(vec![
                "hamishrc line 2: Not a key: <Nope>".to_owned()
            ]))
        );
        keys(cx, "shift-x");
        assert_eq!(text(&workspace, cx), "two");
    });
}

#[test]
fn source_on_the_command_line_reads_the_changed_hamishrc() {
    system_text::run(1, |cx| {
        let (workspace, cx, dir) = open(cx, "one\ntwo", "", true);
        rewrite(&dir, "nnoremap X dd\n");

        keys(cx, ": s o u r c e enter");
        keys(cx, "shift-x");

        assert_eq!(text(&workspace, cx), "two");
    });
}

#[test]
fn reload_hamishrc_reads_the_changed_hamishrc() {
    system_text::run(1, |cx| {
        let (workspace, cx, dir) = open(cx, "one\ntwo", "", true);
        rewrite(&dir, "nnoremap X dd\n");

        cx.dispatch_action(app::ReloadHamishrc);
        cx.run_until_parked();
        keys(cx, "shift-x");

        assert_eq!(text(&workspace, cx), "two");
    });
}

#[test]
fn a_recursive_map_stops_with_a_notice() {
    system_text::run(1, |cx| {
        let (workspace, cx, _dir) = open(cx, "one", "nmap a b\nnmap b a\n", true);

        keys(cx, "a");

        assert_eq!(
            notice(&workspace, cx),
            Some(Notice::Hamishrc(vec![
                "hamishrc: a recursive map stopped after 1000 maps".to_owned()
            ]))
        );
        assert_eq!(text(&workspace, cx), "one");
    });
}

#[test]
fn the_reload_hamishrc_button_of_settings_reads_the_changed_hamishrc() {
    system_text::run(1, |cx| {
        let (workspace, cx, dir) = open(cx, "one\ntwo", "", true);
        rewrite(&dir, "nnoremap X dd\n");

        keys(cx, "ctrl-,");
        for id in ["settings-section-editor", "settings-reload-hamishrc"] {
            let bounds = cx.debug_bounds(id).unwrap();
            cx.simulate_click(bounds.center(), Modifiers::none());
            cx.run_until_parked();
        }
        keys(cx, "escape shift-x");

        assert_eq!(text(&workspace, cx), "two");
    });
}
