use hamish_core::{
    Caret, Document, EditSession, Level, Motion, Provenance, Vim, VimCommand, VimKey, VimMode,
};

/// Text, caret offset, mode and register after Vim keys, with the commands run as the
/// Editor runs them.
struct Run {
    text: String,
    offset: usize,
    mode: VimMode,
    register: String,
}

fn vim_key(key: char) -> VimKey {
    match key {
        '\u{1b}' => VimKey::Escape,
        '\r' => VimKey::Enter,
        '\u{12}' => VimKey::Ctrl('r'),
        key => VimKey::Char(key),
    }
}

/// Runs `keys` from `offset` in `text` with `register` as the register. In `keys`,
/// "\u{1b}" is Escape, "\r" is Enter and "\u{12}" is Ctrl+R. A key that Vim mode passes on
/// types its char at the caret, as the text input does.
fn run_with(text: &str, offset: usize, keys: &str, register: &str) -> Run {
    let mut vim = Vim::default();
    let mut text = text.to_owned();
    let mut caret = Caret::at(offset);
    let mut register = register.to_owned();
    for key in keys.chars() {
        let key = vim_key(key);
        let mut command = vim.key(key, text.as_str(), &caret);
        if let VimCommand::Put(put) = command {
            command = put.command(&register, &text);
        }
        match command {
            VimCommand::PassOn => {
                let VimKey::Char(typed) = key else {
                    continue;
                };
                let at = caret.offset();
                text.insert(at, typed);
                caret = Caret::at(at + typed.len_utf8());
            }
            VimCommand::Nothing
            | VimCommand::Put(_)
            | VimCommand::Undo(_)
            | VimCommand::Redo(_) => {}
            VimCommand::Select(next) => caret = next,
            VimCommand::Edit(edit) => {
                text.replace_range(edit.range, &edit.text);
                caret = edit.caret;
            }
            VimCommand::Cut { edit, removed } => {
                register = Some(removed)
                    .filter(|removed| !removed.is_empty())
                    .unwrap_or(register);
                text.replace_range(edit.range, &edit.text);
                caret = edit.caret;
            }
            VimCommand::Yank {
                text: yanked,
                caret: next,
            } => {
                register = yanked;
                caret = next;
            }
        }
    }
    Run {
        text,
        offset: caret.offset(),
        mode: vim.mode(),
        register,
    }
}

/// The text, the caret offset and the mode after `keys` from `offset` in `text`.
fn run(text: &str, offset: usize, keys: &str) -> (String, usize, VimMode) {
    let done = run_with(text, offset, keys, "");
    (done.text, done.offset, done.mode)
}

/// The text and the caret offset after `keys`.
fn edited(text: &str, offset: usize, keys: &str) -> (String, usize) {
    let done = run_with(text, offset, keys, "");
    (done.text, done.offset)
}

fn offset_after(text: &str, offset: usize, keys: &str) -> usize {
    run(text, offset, keys).1
}

#[test]
fn vim_mode_starts_in_normal_mode_where_letters_type_nothing() {
    let (text, offset, mode) = run("one two", 2, "qzq");
    assert_eq!(
        (text.as_str(), offset, mode),
        ("one two", 2, VimMode::Normal)
    );
}

#[test]
fn h_and_l_move_one_grapheme_and_stay_on_the_line() {
    let text = "one two\nthree";
    assert_eq!(offset_after(text, 4, "l"), 5);
    assert_eq!(offset_after(text, 4, "h"), 3);
    assert_eq!(offset_after(text, 4, "3l"), 6);
    assert_eq!(offset_after(text, 0, "h"), 0);
    assert_eq!(offset_after(text, 8, "h"), 8);
    assert_eq!(offset_after(text, 6, "l"), 6);
    assert_eq!(offset_after("x\u{3b1}\u{301}y", 1, "l"), 5);
}

#[test]
fn zero_and_dollar_go_to_the_first_and_last_char_of_the_line() {
    let text = "one two\nthree\nfour";
    assert_eq!(offset_after(text, 5, "$"), 6);
    assert_eq!(offset_after(text, 5, "0"), 0);
    assert_eq!(offset_after(text, 9, "$"), 12);
    assert_eq!(offset_after(text, 1, "2$"), 12);
}

#[test]
fn w_b_and_e_move_by_words_and_runs_of_punctuation() {
    let text = "one, two  three";
    assert_eq!(offset_after(text, 0, "w"), 3);
    assert_eq!(offset_after(text, 3, "w"), 5);
    assert_eq!(offset_after(text, 5, "w"), 10);
    assert_eq!(offset_after(text, 0, "3w"), 10);
    assert_eq!(offset_after(text, 0, "e"), 2);
    assert_eq!(offset_after(text, 2, "e"), 3);
    assert_eq!(offset_after(text, 3, "e"), 7);
    assert_eq!(offset_after(text, 10, "b"), 5);
    assert_eq!(offset_after(text, 5, "b"), 3);
    assert_eq!(offset_after(text, 3, "b"), 0);
    assert_eq!(offset_after(text, 12, "2b"), 5);
}

#[test]
fn w_stops_at_an_empty_line_and_at_the_last_char() {
    let text = "a\n\nb";
    assert_eq!(offset_after(text, 0, "w"), 2);
    assert_eq!(offset_after(text, 2, "w"), 3);
    assert_eq!(offset_after(text, 3, "b"), 2);
    assert_eq!(offset_after(text, 2, "b"), 0);
    assert_eq!(offset_after("one two", 4, "w"), 6);
}

#[test]
fn words_of_other_scripts_are_words() {
    assert_eq!(offset_after("\u{3b1}\u{301} \u{3b2}\u{3b3}", 0, "w"), 5);
    assert_eq!(offset_after("\u{3b1}\u{301} \u{3b2}\u{3b3}", 0, "e"), 7);
    assert_eq!(offset_after("\u{3b1}\u{301} \u{3b2}\u{3b3}", 5, "e"), 7);
}

#[test]
fn j_and_k_move_by_lines_and_keep_the_column() {
    let text = "abcdef\nab\nabcdef";
    assert_eq!(offset_after(text, 4, "j"), 8);
    assert_eq!(offset_after(text, 4, "jj"), 14);
    assert_eq!(offset_after(text, 4, "2j"), 14);
    assert_eq!(offset_after(text, 14, "kk"), 4);
    assert_eq!(offset_after(text, 4, "k"), 4);
    assert_eq!(offset_after(text, 14, "j"), 14);
    assert_eq!(offset_after(text, 1, "$jj"), 15);
}

#[test]
fn enter_goes_to_the_first_char_of_the_next_line() {
    assert_eq!(offset_after("one\n  two", 2, "\r"), 6);
}

#[test]
fn gg_and_g_go_to_a_line_with_a_count_else_the_first_or_last() {
    let text = "one\n  two\nthree";
    assert_eq!(offset_after(text, 0, "G"), 10);
    assert_eq!(offset_after(text, 12, "gg"), 0);
    assert_eq!(offset_after(text, 12, "2gg"), 6);
    assert_eq!(offset_after(text, 0, "2G"), 6);
    assert_eq!(offset_after(text, 0, "9G"), 10);
}

#[test]
fn a_count_of_more_digits_and_escape_clears_it() {
    let text = "abcdefghijklmnop";
    assert_eq!(offset_after(text, 0, "10l"), 10);
    assert_eq!(offset_after(text, 0, "3\u{1b}l"), 1);
    assert_eq!(offset_after(text, 0, "gql"), 1);
}

#[test]
fn the_status_shows_the_mode_then_the_count_and_the_first_key_of_a_pair() {
    let mut vim = Vim::default();
    let text = "one\ntwo";
    let caret = Caret::at(0);
    assert_eq!(vim.status(), "NORMAL");
    vim.key(VimKey::Char('3'), text, &caret);
    vim.key(VimKey::Char('g'), text, &caret);
    assert_eq!(vim.status(), "NORMAL 3g");

    vim.key(VimKey::Char('g'), text, &caret);
    assert_eq!(vim.status(), "NORMAL");
    vim.key(VimKey::Char('i'), text, &caret);
    assert_eq!(vim.status(), "INSERT");
}

#[test]
fn i_types_before_the_caret_and_escape_steps_back_onto_the_last_char() {
    let (text, offset, mode) = run("ab", 1, "iXY\u{1b}");
    assert_eq!((text.as_str(), offset, mode), ("aXYb", 2, VimMode::Normal));
}

#[test]
fn a_types_after_the_char_at_the_caret() {
    let (text, offset, mode) = run("ab", 1, "aX");
    assert_eq!((text.as_str(), offset, mode), ("abX", 3, VimMode::Insert));
}

#[test]
fn o_and_capital_o_open_a_line_below_and_above() {
    assert_eq!(
        run("ab\ncd", 0, "oX"),
        ("ab\nX\ncd".to_owned(), 4, VimMode::Insert)
    );
    assert_eq!(
        run("ab\ncd", 4, "OX"),
        ("ab\nX\ncd".to_owned(), 4, VimMode::Insert)
    );
    assert_eq!(run("ab", 1, "O"), ("\nab".to_owned(), 0, VimMode::Insert));
}

#[test]
fn in_insert_mode_every_key_but_escape_passes_on() {
    let mut vim = Vim::default();
    let caret = Caret::at(0);
    vim.key(VimKey::Char('i'), "ab", &caret);
    assert_eq!(vim.key(VimKey::Char('j'), "ab", &caret), VimCommand::PassOn);
    assert_eq!(vim.key(VimKey::Enter, "ab", &caret), VimCommand::PassOn);
    assert_eq!(vim.key(VimKey::Other, "ab", &caret), VimCommand::PassOn);
    assert_eq!(vim.mode(), VimMode::Insert);
}

#[test]
fn escape_at_the_line_start_keeps_the_caret() {
    assert_eq!(
        run("ab", 0, "i\u{1b}"),
        ("ab".to_owned(), 0, VimMode::Normal)
    );
}

#[test]
fn the_mode_labels_read_normal_insert_and_visual() {
    assert_eq!(VimMode::Normal.label(), "NORMAL");
    assert_eq!(VimMode::Insert.label(), "INSERT");
    assert_eq!(VimMode::Visual.label(), "VISUAL");
    assert_eq!(VimMode::VisualLine.label(), "VISUAL LINE");
}

#[test]
fn in_normal_mode_a_caret_set_past_the_last_char_settles_on_it() {
    let text = "ab\n\ncd";
    let vim = Vim::default();
    let settled = |caret: Caret| vim.settle(text, caret).selection();
    assert_eq!(settled(Caret::at(2)), 1..1);
    assert_eq!(settled(Caret::at(6)), 5..5);
    assert_eq!(settled(Caret::at(3)), 3..3);
    assert_eq!(settled(Caret::at(1)), 1..1);
    assert_eq!(settled(Caret::over(0..2)), 0..2);
}

#[test]
fn in_insert_mode_a_caret_set_on_the_line_end_stays() {
    let mut vim = Vim::default();
    vim.key(VimKey::Char('i'), "ab", &Caret::at(0));
    assert_eq!(vim.settle("ab", Caret::at(2)).selection(), 2..2);
}

#[test]
fn j_then_k_in_insert_mode_goes_to_normal_mode_and_removes_the_j() {
    assert_eq!(run("", 0, "ixjk"), ("x".to_owned(), 0, VimMode::Normal));
    assert_eq!(run("ab", 0, "ijk"), ("ab".to_owned(), 0, VimMode::Normal));
}

#[test]
fn j_and_k_apart_or_after_a_caret_move_type_as_usual() {
    assert_eq!(run("", 0, "ijxk"), ("jxk".to_owned(), 3, VimMode::Insert));
    assert_eq!(run("", 0, "ikj"), ("kj".to_owned(), 2, VimMode::Insert));
}

#[test]
fn j_then_k_after_the_caret_moved_types_the_k() {
    let mut vim = Vim::default();
    vim.key(VimKey::Char('i'), "", &Caret::at(0));
    assert_eq!(
        vim.key(VimKey::Char('j'), "", &Caret::at(0)),
        VimCommand::PassOn
    );
    assert_eq!(
        vim.key(VimKey::Char('k'), "jaj", &Caret::at(3)),
        VimCommand::PassOn
    );
    assert_eq!(vim.mode(), VimMode::Insert);
}

fn owned((text, offset): (&str, usize)) -> (String, usize) {
    (text.to_owned(), offset)
}

#[test]
fn d_with_a_motion_deletes_to_where_the_motion_goes() {
    let text = "one two three";
    assert_eq!(edited(text, 4, "dw"), owned(("one three", 4)));
    assert_eq!(edited(text, 0, "d2w"), owned(("three", 0)));
    assert_eq!(edited(text, 0, "2dw"), owned(("three", 0)));
    assert_eq!(edited(text, 4, "de"), owned(("one  three", 4)));
    assert_eq!(edited(text, 4, "db"), owned(("two three", 0)));
    assert_eq!(edited(text, 4, "d$"), owned(("one ", 3)));
    assert_eq!(edited(text, 4, "d0"), owned(("two three", 0)));
    assert_eq!(edited(text, 4, "dl"), owned(("one wo three", 4)));
    assert_eq!(edited(text, 4, "dh"), owned(("onetwo three", 3)));
}

#[test]
fn dw_on_the_last_word_of_a_line_keeps_the_newline() {
    assert_eq!(edited("one two\nthree", 4, "dw"), owned(("one \nthree", 3)));
    assert_eq!(edited("one two", 4, "dw"), owned(("one ", 3)));
}

#[test]
fn d_with_a_line_motion_deletes_whole_lines() {
    let text = "one\n  two\nthree\nfour";
    assert_eq!(edited(text, 1, "dj"), owned(("three\nfour", 0)));
    assert_eq!(edited(text, 6, "dk"), owned(("three\nfour", 0)));
    assert_eq!(edited(text, 11, "dG"), owned(("one\n  two", 6)));
    assert_eq!(edited(text, 11, "dgg"), owned(("four", 0)));
}

#[test]
fn dd_deletes_lines_and_the_caret_goes_to_the_first_non_blank_of_the_next() {
    let text = "one\ntwo\n  three";
    assert_eq!(edited(text, 1, "dd"), owned(("two\n  three", 0)));
    assert_eq!(edited(text, 4, "2dd"), owned(("one", 0)));
    assert_eq!(edited(text, 1, "dd"), owned(("two\n  three", 0)));
    assert_eq!(edited(text, 9, "dd"), owned(("one\ntwo", 4)));
    assert_eq!(edited("one", 1, "dd"), owned(("", 0)));
    assert_eq!(edited(text, 0, "dd"), owned(("two\n  three", 0)));
    assert_eq!(edited("a\n  b\nc", 0, "dd"), owned(("  b\nc", 2)));
}

#[test]
fn x_deletes_chars_up_to_the_line_end_and_d_deletes_to_the_line_end() {
    assert_eq!(edited("abc\nd", 1, "x"), owned(("ac\nd", 1)));
    assert_eq!(edited("abc\nd", 2, "x"), owned(("ab\nd", 1)));
    assert_eq!(edited("abc\nd", 1, "5x"), owned(("a\nd", 0)));
    assert_eq!(edited("\nd", 0, "x"), owned(("\nd", 0)));
    assert_eq!(edited("abc\nd", 1, "D"), owned(("a\nd", 0)));
    assert_eq!(edited("ab\ncd\nef", 1, "2D"), owned(("a\nef", 0)));
}

#[test]
fn c_deletes_then_types_in_insert_mode_and_cw_stops_at_the_word_end() {
    assert_eq!(
        run("one two three", 4, "cwsix\u{1b}"),
        ("one six three".to_owned(), 6, VimMode::Normal)
    );
    assert_eq!(
        run("one two", 4, "c$X"),
        ("one X".to_owned(), 5, VimMode::Insert)
    );
    assert_eq!(
        run("one two", 4, "CX"),
        ("one X".to_owned(), 5, VimMode::Insert)
    );
    assert_eq!(
        run("one  two", 0, "c2wX"),
        ("X".to_owned(), 1, VimMode::Insert)
    );
}

#[test]
fn cc_empties_the_lines_and_keeps_the_line() {
    assert_eq!(
        run("one\ntwo\nthree", 5, "ccX\u{1b}"),
        ("one\nX\nthree".to_owned(), 4, VimMode::Normal)
    );
    assert_eq!(
        run("one\ntwo\nthree", 1, "2ccX"),
        ("X\nthree".to_owned(), 1, VimMode::Insert)
    );
}

#[test]
fn y_puts_the_text_in_the_register_and_moves_the_caret_to_its_start() {
    let done = run_with("one two three", 6, "yb", "");
    assert_eq!(
        (done.text.as_str(), done.offset, done.register.as_str()),
        ("one two three", 4, "tw")
    );
    let done = run_with("one two three", 4, "yiw", "");
    assert_eq!((done.offset, done.register.as_str()), (4, "two"));
}

#[test]
fn yy_puts_whole_lines_with_a_newline_after_each() {
    let done = run_with("one\ntwo", 5, "yy", "");
    assert_eq!(
        (done.text.as_str(), done.offset, done.register.as_str()),
        ("one\ntwo", 5, "two\n")
    );
    let done = run_with("one\ntwo\nthree", 1, "2yy", "");
    assert_eq!(done.register, "one\ntwo\n");
}

#[test]
fn d_x_and_c_put_the_deleted_text_in_the_register() {
    assert_eq!(run_with("one two", 4, "dw", "").register, "two");
    assert_eq!(run_with("one\ntwo", 0, "dd", "").register, "one\n");
    assert_eq!(run_with("abc", 1, "x", "").register, "b");
    assert_eq!(run_with("one two", 0, "ciwX", "").register, "one");
    assert_eq!(run_with("\n", 0, "x", "kept").register, "kept");
}

#[test]
fn p_puts_chars_after_the_caret_and_capital_p_before_it() {
    let put = |offset: usize, keys: &str, register: &str| {
        let done = run_with("ab\ncd", offset, keys, register);
        (done.text, done.offset)
    };
    assert_eq!(put(0, "p", "XY"), owned(("aXYb\ncd", 2)));
    assert_eq!(put(0, "P", "XY"), owned(("XYab\ncd", 1)));
    assert_eq!(put(1, "2p", "X"), owned(("abXX\ncd", 3)));
    assert_eq!(put(0, "p", ""), owned(("ab\ncd", 0)));
    let done = run_with("\nz", 0, "p", "X");
    assert_eq!((done.text.as_str(), done.offset), ("X\nz", 0));
}

#[test]
fn p_puts_lines_below_the_line_and_capital_p_above_it() {
    let put = |offset: usize, keys: &str| {
        let done = run_with("ab\ncd", offset, keys, "  X\n");
        (done.text, done.offset)
    };
    assert_eq!(put(0, "p"), owned(("ab\n  X\ncd", 5)));
    assert_eq!(put(4, "p"), owned(("ab\ncd\n  X", 8)));
    assert_eq!(put(4, "P"), owned(("ab\n  X\ncd", 5)));
    assert_eq!(put(0, "2P"), owned(("  X\n  X\nab\ncd", 2)));
}

#[test]
fn dd_then_p_moves_a_line_down() {
    assert_eq!(
        edited("one\ntwo\nthree", 0, "ddp"),
        owned(("two\none\nthree", 4))
    );
}

#[test]
fn text_objects_iw_and_aw_take_a_word_with_or_without_its_blanks() {
    let text = "one two  three";
    assert_eq!(edited(text, 5, "diw"), owned(("one   three", 4)));
    assert_eq!(edited(text, 5, "daw"), owned(("one three", 4)));
    assert_eq!(edited(text, 10, "daw"), owned(("one two", 6)));
    assert_eq!(edited(text, 3, "diw"), owned(("onetwo  three", 3)));
    assert_eq!(edited(text, 0, "d3iw"), owned(("  three", 0)));
    assert_eq!(edited(text, 0, "d2aw"), owned(("three", 0)));
    assert_eq!(edited("a\n\nb", 2, "diw"), owned(("a\n\nb", 2)));
}

#[test]
fn text_objects_is_and_as_take_a_sentence() {
    let text = "One two. Three four. Five.";
    assert_eq!(edited(text, 12, "dis"), owned(("One two.  Five.", 9)));
    assert_eq!(edited(text, 12, "das"), owned(("One two. Five.", 9)));
    assert_eq!(edited(text, 22, "das"), owned(("One two. Three four.", 19)));
}

#[test]
fn text_objects_ip_and_ap_take_whole_lines_of_a_paragraph() {
    let text = "a\nb\n\nc\n\nd";
    assert_eq!(edited(text, 2, "dip"), owned(("\nc\n\nd", 0)));
    assert_eq!(edited(text, 2, "dap"), owned(("c\n\nd", 0)));
    assert_eq!(edited(text, 9, "dap"), owned(("a\nb\n\nc", 5)));
    assert_eq!(edited(text, 0, "y2ip"), owned((text, 0)));
    assert_eq!(run_with(text, 0, "y2ip", "").register, "a\nb\n\n");
}

/// The Document "one two three" with a Word Slot over "two" (Variants "two", "2") and a
/// Ghost range over "three".
fn slot_and_ghost() -> Document {
    let mut document = Document::new("one two three");
    assert!(
        document
            .create_slot(4..7, Level::Word, "2", Provenance::Writer)
            .is_ok()
    );
    assert!(document.ghost(8..13).is_ok());
    document
}

#[test]
fn text_objects_iv_and_ig_take_the_active_variant_and_the_ghost_range() {
    let document = slot_and_ghost();
    let mut vim = Vim::default();
    let caret = Caret::at(5);
    vim.key(VimKey::Char('d'), &document, &caret);
    vim.key(VimKey::Char('i'), &document, &caret);
    let VimCommand::Cut { edit, removed } = vim.key(VimKey::Char('v'), &document, &caret) else {
        panic!("diw gives a cut");
    };
    assert_eq!((edit.range, removed.as_str()), (4..7, "two"));

    let caret = Caret::at(9);
    vim.key(VimKey::Char('y'), &document, &caret);
    vim.key(VimKey::Char('i'), &document, &caret);
    assert_eq!(
        vim.key(VimKey::Char('g'), &document, &caret),
        VimCommand::Yank {
            text: "three".to_owned(),
            caret: Caret::at(8)
        }
    );

    vim.key(VimKey::Char('d'), &document, &Caret::at(0));
    vim.key(VimKey::Char('i'), &document, &Caret::at(0));
    assert_eq!(
        vim.key(VimKey::Char('v'), &document, &Caret::at(0)),
        VimCommand::Nothing
    );
}

#[test]
fn the_status_shows_an_operator_and_its_count() {
    let mut vim = Vim::default();
    let caret = Caret::at(0);
    for key in "2d3i".chars() {
        vim.key(VimKey::Char(key), "one two", &caret);
    }
    assert_eq!(vim.status(), "NORMAL 2d3i");
    vim.key(VimKey::Escape, "one two", &caret);
    assert_eq!(vim.status(), "NORMAL");
}

#[test]
fn an_operator_with_a_key_that_is_no_motion_does_nothing() {
    assert_eq!(edited("one two", 0, "dqx"), owned(("ne two", 0)));
    assert_eq!(edited("one two", 0, "d\u{1b}x"), owned(("ne two", 0)));
}

#[test]
fn v_selects_from_the_char_where_it_started_to_the_caret_char() {
    let mut vim = Vim::default();
    let text = "one two three";
    let caret = Caret::at(4);
    assert_eq!(
        vim.key(VimKey::Char('v'), text, &caret),
        VimCommand::Select(Caret::over(4..5))
    );
    assert_eq!(vim.mode(), VimMode::Visual);
    assert_eq!(
        vim.key(VimKey::Char('e'), text, &caret),
        VimCommand::Select(Caret::over(4..7))
    );
    assert_eq!(
        vim.key(VimKey::Char('2'), text, &caret),
        VimCommand::Nothing
    );
    let VimCommand::Select(back) = vim.key(VimKey::Char('b'), text, &caret) else {
        panic!("b selects")
    };
    assert_eq!((back.selection(), back.is_reversed()), (0..5, true));
    assert_eq!(vim.status(), "VISUAL");
}

#[test]
fn in_visual_mode_operators_act_on_the_selection_and_go_back_to_normal_mode() {
    assert_eq!(
        run("one two three", 4, "vedx"),
        ("one three".to_owned(), 4, VimMode::Normal)
    );
    assert_eq!(
        run("one two three", 4, "vecX\u{1b}"),
        ("one X three".to_owned(), 4, VimMode::Normal)
    );
    let done = run_with("one two three", 8, "vby", "");
    assert_eq!(
        (done.offset, done.register.as_str(), done.mode),
        (4, "two t", VimMode::Normal)
    );
    assert_eq!(
        run("one two", 4, "vl\u{1b}"),
        ("one two".to_owned(), 5, VimMode::Normal)
    );
    assert_eq!(
        run("one two", 4, "vv"),
        ("one two".to_owned(), 4, VimMode::Normal)
    );
}

#[test]
fn capital_v_selects_whole_lines() {
    assert_eq!(
        edited("one\ntwo\nthree\nfour", 5, "Vjd"),
        owned(("one\nfour", 4))
    );
    let done = run_with("one\ntwo", 1, "Vy", "");
    assert_eq!(
        (done.register.as_str(), done.mode),
        ("one\n", VimMode::Normal)
    );
    let mut vim = Vim::default();
    let caret = Caret::at(5);
    vim.key(VimKey::Char('V'), "one\ntwo\nthree", &caret);
    assert_eq!(vim.mode(), VimMode::VisualLine);
    assert_eq!(
        vim.key(VimKey::Char('k'), "one\ntwo\nthree", &caret),
        VimCommand::Select(Caret::at(7).extended("one\ntwo\nthree", Motion::To(0)))
    );
}

#[test]
fn in_visual_mode_a_text_object_selects_it() {
    assert_eq!(edited("one two three", 5, "viwd"), owned(("one  three", 4)));
    assert_eq!(edited("a\nb\n\nc", 0, "vipd"), owned(("\nc", 0)));
    let done = run_with("one two", 5, "viwp", "six");
    assert_eq!(
        (done.text.as_str(), done.register.as_str()),
        ("one six", "two")
    );
}

#[test]
fn dot_repeats_the_last_change() {
    assert_eq!(edited("a b c d", 0, "dw.."), owned(("d", 0)));
    assert_eq!(edited("abcdef", 0, "2x."), owned(("ef", 0)));
    assert_eq!(edited("abcdef", 0, "2x3."), owned(("f", 0)));
    assert_eq!(edited("one\ntwo\nthree", 0, "ddj."), owned(("two", 0)));
    assert_eq!(edited("one two", 0, "w."), owned(("one two", 4)));
}

#[test]
fn dot_repeats_a_change_with_the_text_typed_in_insert_mode() {
    assert_eq!(
        run("one two three", 0, "ciwsix\u{1b}ww."),
        ("six two six".to_owned(), 10, VimMode::Normal)
    );
    assert_eq!(
        run("a\nb", 0, "oX\u{1b}j."),
        ("a\nX\nb\nX".to_owned(), 6, VimMode::Normal)
    );
    assert_eq!(
        run("ab", 0, "iXY\u{1b}."),
        ("XXYYab".to_owned(), 2, VimMode::Normal)
    );
    assert_eq!(
        run("one two", 0, "cwsixjkw."),
        ("six six".to_owned(), 6, VimMode::Normal)
    );
}

#[test]
fn dot_repeats_a_put() {
    let done = run_with("ab", 0, "p.", "X");
    assert_eq!((done.text.as_str(), done.offset), ("aXXb", 2));
}

#[test]
fn u_and_ctrl_r_ask_for_undo_and_redo_with_a_count() {
    let mut vim = Vim::default();
    let caret = Caret::at(0);
    assert_eq!(
        vim.key(VimKey::Char('u'), "ab", &caret),
        VimCommand::Undo(1)
    );
    vim.key(VimKey::Char('3'), "ab", &caret);
    assert_eq!(
        vim.key(VimKey::Ctrl('r'), "ab", &caret),
        VimCommand::Redo(3)
    );
    vim.key(VimKey::Char('i'), "ab", &caret);
    assert_eq!(vim.key(VimKey::Ctrl('r'), "ab", &caret), VimCommand::PassOn);
}

/// Runs `keys` on `session` as the Editor does; the register stays empty.
fn run_session(session: &mut EditSession, vim: &mut Vim, keys: &str) {
    for key in keys.chars() {
        let key = vim_key(key);
        let command = vim.key(key, session.document(), session.caret());
        match command {
            VimCommand::PassOn => {
                let VimKey::Char(typed) = key else {
                    continue;
                };
                let edit =
                    session
                        .caret()
                        .input(session.document().text(), None, &typed.to_string());
                session.edit(&edit);
            }
            VimCommand::Select(caret) | VimCommand::Yank { caret, .. } => session.select(caret),
            VimCommand::Edit(edit) | VimCommand::Cut { edit, .. } => {
                session.edit(&edit);
            }
            VimCommand::Undo(times) => {
                for _ in 0..times {
                    session.undo();
                }
            }
            VimCommand::Redo(times) => {
                for _ in 0..times {
                    session.redo();
                }
            }
            VimCommand::Nothing | VimCommand::Put(_) => {}
        }
    }
}

#[test]
fn one_vim_command_is_one_undo_step() {
    let mut session = EditSession::new(Document::new("one two three four"));
    let mut vim = Vim::default();
    run_session(&mut session, &mut vim, "d2wciwsix\u{1b}xx");
    assert_eq!(session.document().text(), "sifour");

    run_session(&mut session, &mut vim, "u");
    assert_eq!(session.document().text(), "si four");
    run_session(&mut session, &mut vim, "2u");
    assert_eq!(session.document().text(), "three four");
    run_session(&mut session, &mut vim, "u");
    assert_eq!(session.document().text(), "one two three four");
    run_session(&mut session, &mut vim, "\u{12}");
    assert_eq!(session.document().text(), "three four");
}

#[test]
fn a_change_with_typing_and_deleting_in_insert_mode_is_one_undo_step() {
    let mut session = EditSession::new(Document::new("one two"));
    let mut vim = Vim::default();
    run_session(&mut session, &mut vim, "ciwsx");
    let edit = session
        .caret()
        .delete(session.document().text(), Motion::Left);
    session.edit(&edit);
    run_session(&mut session, &mut vim, "ix\u{1b}");
    assert_eq!(session.document().text(), "six two");

    run_session(&mut session, &mut vim, "u");
    assert_eq!(session.document().text(), "one two");
}

#[test]
fn dot_repeat_is_one_undo_step() {
    let mut session = EditSession::new(Document::new("one two"));
    let mut vim = Vim::default();
    run_session(&mut session, &mut vim, "ciwsix\u{1b}w.");
    assert_eq!(session.document().text(), "six six");
    run_session(&mut session, &mut vim, "u");
    assert_eq!(session.document().text(), "six two");
}

#[test]
fn an_operator_over_a_slot_edge_edits_as_a_selection_delete_does() {
    let mut by_vim = EditSession::new(slot_and_ghost());
    let mut vim = Vim::default();
    by_vim.select(Caret::at(2));
    run_session(&mut by_vim, &mut vim, "vlld");

    let mut by_selection = EditSession::new(slot_and_ghost());
    by_selection.select(Caret::over(2..5));
    let edit = by_selection
        .caret()
        .delete(by_selection.document().text(), Motion::Left);
    by_selection.edit(&edit);

    assert_eq!(by_vim.document().text(), by_selection.document().text());
    assert_eq!(by_vim.document().slots(), by_selection.document().slots());
    assert_eq!(by_vim.document().text(), "onwo three");
}
