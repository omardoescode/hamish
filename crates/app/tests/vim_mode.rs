use std::ops::Range;

use app::Workspace;
use gpui::{Entity, TestAppContext, VisualTestContext};
use hamish_core::{Document, LoadedSettings, Settings};

fn set_vim_mode(cx: &mut VisualTestContext, vim_mode: bool) {
    cx.update(|_, cx| {
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
    });
    cx.run_until_parked();
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    vim_mode: bool,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let document = Document::new(text);
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    set_vim_mode(cx, vim_mode);
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

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range<usize> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).selection())
}

fn status(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.read(|cx| workspace.read(cx).vim_status(cx))
}

#[gpui::test]
fn with_vim_mode_off_letters_type_and_the_chrome_shows_no_mode(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "ab", false);

    cx.simulate_keystrokes("j k");

    assert_eq!(text(&workspace, cx), "jkab");
    assert_eq!(status(&workspace, cx), None);
    assert!(cx.debug_bounds("vim-mode").is_none());
}

#[gpui::test]
fn in_normal_mode_vim_keys_move_the_caret_and_type_nothing(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "one two\nthree four\nfive", true);
    assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));

    cx.simulate_keystrokes("j w");
    assert_eq!(selection(&workspace, cx), 14..14);

    cx.simulate_keystrokes("$");
    assert_eq!(selection(&workspace, cx), 17..17);

    cx.simulate_keystrokes("g g");
    assert_eq!(selection(&workspace, cx), 0..0);

    cx.simulate_keystrokes("2 j 0");
    assert_eq!(selection(&workspace, cx), 19..19);

    cx.simulate_keystrokes("shift-g k w b e");
    assert_eq!(selection(&workspace, cx), 12..12);

    cx.simulate_keystrokes("q enter backspace delete");
    assert_eq!(text(&workspace, cx), "one two\nthree four\nfive");
}

#[gpui::test]
fn the_chrome_bar_shows_the_mode_and_the_pending_keys(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "one\ntwo", true);

    cx.simulate_keystrokes("3 g");
    assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL 3g"));
    let bar = cx.debug_bounds("chrome-bar");
    let mode = cx.debug_bounds("vim-mode");
    assert!(
        bar.zip(mode)
            .is_some_and(|(bar, mode)| bar.top() <= mode.top() && mode.bottom() <= bar.bottom()),
        "the mode {mode:?} is in the Chrome bar {bar:?}"
    );

    cx.simulate_keystrokes("g i");
    assert_eq!(status(&workspace, cx).as_deref(), Some("INSERT"));
}

#[gpui::test]
fn i_types_until_escape_and_escape_steps_back(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "ab", true);

    cx.simulate_keystrokes("l i");
    cx.simulate_input("XY");
    assert_eq!(text(&workspace, cx), "aXYb");

    cx.simulate_keystrokes("escape");
    assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
    assert_eq!(selection(&workspace, cx), 2..2);

    cx.simulate_keystrokes("j");
    assert_eq!(text(&workspace, cx), "aXYb");
}

#[gpui::test]
fn o_opens_a_line_and_insert_mode_keeps_the_ctrl_keys(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "ab\ncd", true);

    cx.simulate_keystrokes("o");
    cx.simulate_input("X");
    assert_eq!(text(&workspace, cx), "ab\nX\ncd");

    cx.simulate_keystrokes("ctrl-a backspace");
    assert_eq!(text(&workspace, cx), "");

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&workspace, cx), "ab\nX\ncd");
}

#[gpui::test]
fn turning_vim_mode_off_gives_back_the_usual_keys(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, "ab", true);
    cx.simulate_keystrokes("i");

    set_vim_mode(cx, false);
    cx.simulate_keystrokes("j");
    assert_eq!(text(&workspace, cx), "jab");

    set_vim_mode(cx, true);
    assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
}
