use hamish_core::{Caret, Vim, VimCommand, VimKey, VimMode};

/// The text, the caret offset and the mode after `keys` from `offset` in `text`. In
/// `keys`, "\u{1b}" is Escape and "\r" is Enter. A key that Vim mode passes on types its
/// char at the caret, as the text input does.
fn run(text: &str, offset: usize, keys: &str) -> (String, usize, VimMode) {
    let mut vim = Vim::default();
    let mut text = text.to_owned();
    let mut caret = Caret::at(offset);
    for key in keys.chars() {
        let key = match key {
            '\u{1b}' => VimKey::Escape,
            '\r' => VimKey::Enter,
            key => VimKey::Char(key),
        };
        match vim.key(key, &text, &caret) {
            VimCommand::PassOn => {
                let VimKey::Char(typed) = key else {
                    continue;
                };
                let at = caret.offset();
                text.insert(at, typed);
                caret = Caret::at(at + typed.len_utf8());
            }
            VimCommand::Nothing => {}
            VimCommand::Select(next) => caret = next,
            VimCommand::Edit(edit) => {
                text.replace_range(edit.range, &edit.text);
                caret = edit.caret;
            }
        }
    }
    (text, caret.offset(), vim.mode())
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
fn the_mode_labels_read_normal_and_insert() {
    assert_eq!(VimMode::Normal.label(), "NORMAL");
    assert_eq!(VimMode::Insert.label(), "INSERT");
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
