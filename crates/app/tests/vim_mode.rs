mod system_text;

use std::ops::Range;

use app::Workspace;
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, Pixels, TestAppContext, VisualTestContext,
    point, px,
};
use hamish_core::{Document, Level, LoadedSettings, Provenance, Settings};

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
    open_document(cx, Document::new(text), vim_mode)
}

fn open_document(
    cx: &mut TestAppContext,
    document: Document,
    vim_mode: bool,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
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

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[test]
fn d_and_y_put_text_in_the_system_clipboard_and_p_puts_it_back() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two three", true);

        cx.simulate_keystrokes("d w");
        assert_eq!(text(&workspace, cx), "two three");
        assert_eq!(clipboard(cx).as_deref(), Some("one "));

        cx.simulate_keystrokes("$ p");
        assert_eq!(text(&workspace, cx), "two threeone ");

        cx.simulate_keystrokes("0 y i w");
        assert_eq!(clipboard(cx).as_deref(), Some("two"));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("X".to_owned()));
        cx.simulate_keystrokes("shift-p");
        assert_eq!(text(&workspace, cx), "Xtwo threeone ");
    });
}

#[test]
fn u_and_ctrl_r_undo_and_redo_one_vim_command_at_a_time() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two", true);

        cx.simulate_keystrokes("c i w");
        cx.simulate_input("six");
        cx.simulate_keystrokes("escape w x");
        assert_eq!(text(&workspace, cx), "six wo");

        cx.simulate_keystrokes("u");
        assert_eq!(text(&workspace, cx), "six two");
        cx.simulate_keystrokes("u");
        assert_eq!(text(&workspace, cx), "one two");
        assert_eq!(selection(&workspace, cx), 0..0);

        cx.simulate_keystrokes("ctrl-r");
        assert_eq!(text(&workspace, cx), "six two");
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
    });
}

#[test]
fn visual_mode_selects_and_types_nothing_and_d_deletes_the_selection() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two\nthree", true);

        cx.simulate_keystrokes("v e");
        assert_eq!(status(&workspace, cx).as_deref(), Some("VISUAL"));
        assert_eq!(selection(&workspace, cx), 0..3);

        cx.simulate_keystrokes("q delete");
        cx.simulate_input("z");
        assert_eq!(selection(&workspace, cx), 0..3);
        assert_eq!(text(&workspace, cx), "one two\nthree");

        cx.simulate_keystrokes("shift-v");
        assert_eq!(status(&workspace, cx).as_deref(), Some("VISUAL LINE"));
        cx.simulate_keystrokes("d");
        assert_eq!(text(&workspace, cx), "three");
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));

        cx.simulate_keystrokes("v escape");
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
    });
}

#[test]
fn dot_repeats_the_last_change_in_the_editor() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two three", true);

        cx.simulate_keystrokes("c w");
        cx.simulate_input("six");
        cx.simulate_keystrokes("escape w .");
        assert_eq!(text(&workspace, cx), "six six three");
    });
}

fn ghost_ranges(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Range<usize>> {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .ghost_ranges()
    })
}

#[test]
fn gh_ghosts_and_capital_gh_revives_as_one_undo_step_each() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two three", true);

        cx.simulate_keystrokes("w g h i w");
        assert_eq!(ghost_ranges(&workspace, cx), vec![4..7]);
        assert_eq!(selection(&workspace, cx), 4..4);

        cx.simulate_keystrokes("g shift-h i w");
        assert_eq!(ghost_ranges(&workspace, cx), Vec::<Range<usize>>::new());

        cx.simulate_keystrokes("u");
        assert_eq!(ghost_ranges(&workspace, cx), vec![4..7]);
        assert_eq!(text(&workspace, cx), "one two three");
    });
}

#[test]
fn gs_stashes_the_visual_selection_and_shows_overflow() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two three", true);

        cx.simulate_keystrokes("w v e g s");

        assert_eq!(text(&workspace, cx), "one  three");
        assert_eq!(status(&workspace, cx).as_deref(), Some("NORMAL"));
        assert!(cx.read(|cx| workspace.read(cx).overflow_open()));
        let overflow = cx.read(|cx| {
            workspace
                .read(cx)
                .editor()
                .read(cx)
                .document()
                .overflow()
                .text()
                .to_owned()
        });
        assert_eq!(overflow, "two");
    });
}

/// "one two three" with a Word Slot over "three" (Variants "three", "3").
fn slot_on_three() -> Document {
    let mut document = Document::new("one two three");
    assert!(
        document
            .create_slot(8..13, Level::Word, "3", Provenance::Writer)
            .is_ok()
    );
    document
}

#[test]
fn bracket_s_jumps_to_the_slot_and_bracket_v_cycles_it() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open_document(cx, slot_on_three(), true);

        cx.simulate_keystrokes("] s");
        assert_eq!(selection(&workspace, cx), 8..8);

        cx.simulate_keystrokes("] v");
        assert_eq!(text(&workspace, cx), "one two 3");

        cx.simulate_keystrokes("2 [ v");
        assert_eq!(text(&workspace, cx), "one two 3");

        cx.simulate_keystrokes("[ v");
        assert_eq!(text(&workspace, cx), "one two three");

        cx.simulate_keystrokes("u");
        assert_eq!(text(&workspace, cx), "one two 3");
    });
}

#[test]
fn space_a_opens_the_alternatives_panel_on_the_word_at_the_caret() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two", true);

        cx.simulate_keystrokes("w space a");
        cx.run_until_parked();

        let rows = cx.read(|cx| {
            workspace
                .read(cx)
                .alternatives_panel()
                .map(|panel| panel.read(cx).rows(cx))
        });
        assert_eq!(
            rows.map(|rows| rows.into_iter().map(|row| row.text).collect::<Vec<_>>()),
            Some(vec!["two".to_owned()])
        );
    });
}

#[test]
fn space_l_opens_the_lab() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, "one two", true);

        cx.simulate_keystrokes("space l");
        cx.run_until_parked();

        assert!(cx.read(|cx| workspace.read(cx).lab().read(cx).is_open()));
    });
}
