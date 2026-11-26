mod system_text;

use std::ops::Range;

use app::Workspace;
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, Pixels, TestAppContext, VisualTestContext,
    point, px,
};
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

#[test]
fn with_vim_mode_off_letters_type_and_the_chrome_shows_no_mode() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "ab", false);

        cx.simulate_keystrokes("j k");

        assert_eq!(text(&workspace, cx), "jkab");
        assert_eq!(status(&workspace, cx), None);
        assert!(cx.debug_bounds("vim-mode").is_none());
    });
}

#[test]
fn in_normal_mode_vim_keys_move_the_caret_and_type_nothing() {
    system_text::run(1, |cx| {
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
    });
}

#[test]
fn the_chrome_bar_shows_the_mode_and_the_pending_keys() {
    system_text::run(1, |cx| {
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
    });
}

#[test]
fn i_types_until_escape_and_escape_steps_back() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "ab", true);

        cx.simulate_keystrokes("l i");
        cx.simulate_input("XY");
        assert_eq!(text(&workspace, cx), "aXYb");

        cx.simulate_keystrokes("escape");
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
        assert_eq!(selection(&workspace, cx), 2..2);

        cx.simulate_keystrokes("j");
        assert_eq!(text(&workspace, cx), "aXYb");
    });
}

#[test]
fn o_opens_a_line_and_insert_mode_keeps_the_ctrl_keys() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "ab\ncd", true);

        cx.simulate_keystrokes("o");
        cx.simulate_input("X");
        assert_eq!(text(&workspace, cx), "ab\nX\ncd");

        cx.simulate_keystrokes("ctrl-a backspace");
        assert_eq!(text(&workspace, cx), "");

        cx.simulate_keystrokes("ctrl-z");
        assert_eq!(text(&workspace, cx), "ab\nX\ncd");
    });
}

#[test]
fn turning_vim_mode_off_gives_back_the_usual_keys() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "ab", true);
        cx.simulate_keystrokes("i");

        set_vim_mode(cx, false);
        cx.simulate_keystrokes("j");
        assert_eq!(text(&workspace, cx), "jab");

        set_vim_mode(cx, true);
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
    });
}

#[expect(clippy::unwrap_used, reason = "test fixture")]
fn bounds_at(
    workspace: &Entity<Workspace>,
    offset: usize,
    cx: &mut VisualTestContext,
) -> Bounds<Pixels> {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.bounds_for_range(offset..offset, Bounds::default(), window, cx)
        })
    })
    .unwrap()
}

fn caret_width(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Pixels> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.read(|cx| workspace.read(cx).editor().read(cx).caret_bounds())
        .map(|bounds| bounds.size.width)
}

#[test]
fn normal_mode_shows_a_block_caret_as_wide_as_the_char() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "mi\n\nx", true);

        let on_m = caret_width(&workspace, cx).unwrap_or_default();
        cx.simulate_keystrokes("l");
        let on_i = caret_width(&workspace, cx).unwrap_or_default();
        cx.simulate_keystrokes("j");
        assert_eq!(selection(&workspace, cx), 3..3);
        let on_empty_line = caret_width(&workspace, cx).unwrap_or_default();

        assert!(on_i > px(3.), "block on i: {on_i:?}");
        let m = bounds_at(&workspace, 1, cx).origin.x - bounds_at(&workspace, 0, cx).origin.x;
        assert!(
            (on_m - m).abs() < px(0.5),
            "block on m {on_m:?}, m is {m:?} wide"
        );
        assert!(
            on_empty_line > px(3.),
            "block on an empty line: {on_empty_line:?}"
        );
    });
}

#[test]
fn insert_mode_and_vim_mode_off_show_the_thin_bar() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "mi", true);

        cx.simulate_keystrokes("i");
        let insert = caret_width(&workspace, cx).unwrap_or_default();
        set_vim_mode(cx, false);
        let off = caret_width(&workspace, cx).unwrap_or_default();

        assert!(
            px(1.9) < insert && insert < px(2.5),
            "bar in Insert mode: {insert:?}"
        );
        assert!(
            px(1.9) < off && off < px(2.5),
            "bar with Vim mode off: {off:?}"
        );
    });
}

fn click_past_the_end_of_the_first_line(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let end = bounds_at(workspace, 2, cx);
    cx.simulate_click(
        point(end.origin.x + px(40.), end.center().y),
        Modifiers::none(),
    );
}

#[test]
fn in_normal_mode_a_click_past_the_line_end_puts_the_caret_on_the_last_char() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "ab\n\ncd", true);

        click_past_the_end_of_the_first_line(&workspace, cx);
        assert_eq!(selection(&workspace, cx), 1..1);

        cx.simulate_keystrokes("i");
        click_past_the_end_of_the_first_line(&workspace, cx);
        assert_eq!(selection(&workspace, cx), 2..2);
    });
}

#[test]
fn j_then_k_in_insert_mode_goes_to_normal_mode_without_the_j() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "ab", true);

        cx.simulate_keystrokes("a x j k");

        assert_eq!(text(&workspace, cx), "axb");
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
        assert_eq!(selection(&workspace, cx), 1..1);
    });
}
