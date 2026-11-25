use hamish_core::{Caret, CaretEdit, EditKind, Motion, byte_range, utf16_range, word_at};

/// "ά" is one grapheme of two chars and four bytes.
const ALPHA_TONOS: &str = "x\u{3b1}\u{301}y";

#[test]
fn left_and_right_step_over_one_whole_grapheme() {
    let right = Caret::at(1).moved(ALPHA_TONOS, Motion::Right);
    assert_eq!(right.offset(), 5);

    let left = right.moved(ALPHA_TONOS, Motion::Left);
    assert_eq!(left.offset(), 1);
}

#[test]
fn extending_past_the_anchor_flips_the_selection_and_left_collapses_it_to_its_start() {
    let text = "abcdef";
    let back = Caret::at(3)
        .extended(text, Motion::Left)
        .extended(text, Motion::Left);
    assert_eq!(back.selection(), 1..3);
    assert_eq!(back.offset(), 1);

    let flipped = back.extended(text, Motion::To(5));
    assert_eq!(flipped.selection(), 3..5);
    assert_eq!(flipped.offset(), 5);

    assert_eq!(flipped.moved(text, Motion::Left), Caret::at(3));
}

#[test]
fn select_all_puts_the_caret_at_the_end() {
    let all = Caret::all("abc");
    assert_eq!(all.selection(), 0..3);
    assert_eq!(all.offset(), 3);
}

#[test]
fn backspace_deletes_the_grapheme_before_the_caret_as_deleting() {
    let edit = Caret::at(5).delete(ALPHA_TONOS, Motion::Left);
    assert_eq!(
        edit,
        CaretEdit {
            range: 1..5,
            text: String::new(),
            kind: EditKind::Deleting,
            caret: Caret::at(1),
        }
    );
}

#[test]
fn a_delete_over_a_selection_deletes_the_selection_and_joins_no_other_edit() {
    let selected = Caret::at(1).extended("abcd", Motion::To(3));
    let edit = selected.delete("abcd", Motion::Right);
    assert_eq!(edit.range, 1..3);
    assert_eq!(edit.kind, EditKind::Other);
    assert_eq!(edit.caret, Caret::at(1));
}

#[test]
fn an_insert_replaces_the_selection_and_the_caret_goes_after_it() {
    let selected = Caret::at(1).extended("abcd", Motion::To(3));
    let edit = selected.insert("abcd", "\n");
    assert_eq!(edit.range, 1..3);
    assert_eq!(edit.text, "\n");
    assert_eq!(edit.kind, EditKind::Other);
    assert_eq!(edit.caret, Caret::at(2));
}

/// "𝛂" takes four UTF-8 bytes and two UTF-16 units.
const WIDE_ALPHA: &str = "a\u{1d6c2}b";

#[test]
fn utf16_ranges_count_a_wide_char_as_two_units() {
    assert_eq!(utf16_range(WIDE_ALPHA, &(1..5)), 1..3);
    assert_eq!(byte_range(WIDE_ALPHA, &(1..3)), 1..5);
    assert_eq!(byte_range(WIDE_ALPHA, &(2..9)), 5..6);
    let selected = Caret::at(1).extended(WIDE_ALPHA, Motion::Right);
    assert_eq!(selected.selected_utf16(WIDE_ALPHA), 1..3);
}

#[test]
fn input_at_the_caret_is_typing_and_input_over_a_range_joins_no_other_edit() {
    let typed = Caret::at(6).input(WIDE_ALPHA, None, "c");
    assert_eq!(typed, CaretEdit::new(6..6, "c", EditKind::Typing));

    let replaced = Caret::at(0).input(WIDE_ALPHA, Some(3..4), "X");
    assert_eq!(replaced, CaretEdit::new(5..6, "X", EditKind::Other));
}

#[test]
fn composed_text_is_marked_and_its_selection_counts_utf16_inside_it() {
    let composed = Caret::at(2).compose("ab", None, "\u{1d6c2}\u{1d6c2}", Some(2..2));
    assert_eq!(composed.range, 2..2);
    assert_eq!(composed.kind, EditKind::Typing);
    assert_eq!(composed.caret.selection(), 6..6);
    assert_eq!(composed.caret.marked(), Some(2..10));

    let text = "ab\u{1d6c2}\u{1d6c2}";
    assert_eq!(composed.caret.marked_utf16(text), Some(2..6));
    let committed = composed.caret.input(text, None, "Z");
    assert_eq!(committed, CaretEdit::new(2..10, "Z", EditKind::Other));
}

#[test]
fn a_caret_past_or_inside_a_char_of_a_changed_text_clamps_before_it_acts() {
    assert_eq!(Caret::at(10).moved("ab", Motion::Left), Caret::at(1));
    assert_eq!(
        Caret::at(3).insert(WIDE_ALPHA, "x"),
        CaretEdit::new(1..1, "x", EditKind::Other)
    );
}

#[test]
fn a_followed_selection_moves_with_a_change_before_it_and_drops_composed_text() {
    let caret = Caret::at(1).compose("I a cat.", None, "x", None).caret;
    let selected = Caret::at(4).extended("It a cat.", Motion::To(7));

    assert_eq!(caret.followed("Ix a cat.", "Ix  cat.").marked(), None);
    let followed = selected.followed("It a cat.", "It is a cat.");
    assert_eq!(followed.selection(), 7..10);
}

#[test]
fn word_moves_go_to_the_start_of_the_word_before_and_the_end_of_the_word_after() {
    let text = "The red, old cat.";
    assert_eq!(Caret::at(10).moved(text, Motion::WordLeft), Caret::at(9));
    assert_eq!(Caret::at(9).moved(text, Motion::WordLeft), Caret::at(4));
    assert_eq!(Caret::at(2).moved(text, Motion::WordLeft), Caret::at(0));
    assert_eq!(Caret::at(4).moved(text, Motion::WordRight), Caret::at(7));
    assert_eq!(Caret::at(7).moved(text, Motion::WordRight), Caret::at(12));
    assert_eq!(Caret::at(16).moved(text, Motion::WordRight), Caret::at(17));
}

#[test]
fn line_moves_stop_at_the_newlines_around_the_caret() {
    let text = "Title\nThe cat sat.\nEnd";
    assert_eq!(Caret::at(10).moved(text, Motion::LineStart), Caret::at(6));
    assert_eq!(Caret::at(10).moved(text, Motion::LineEnd), Caret::at(18));
    assert_eq!(Caret::at(2).moved(text, Motion::LineStart), Caret::at(0));
    assert_eq!(Caret::at(20).moved(text, Motion::LineEnd), Caret::at(22));
}

#[test]
fn a_word_move_extends_a_selection_from_its_caret_end() {
    let selected = Caret::at(4).extended("The red cat.", Motion::WordRight);
    assert_eq!(selected.selection(), 4..7);
}

#[test]
fn the_word_at_an_offset_selects_it_and_a_gap_gives_the_caret() {
    let text = "The customer waits.";
    assert_eq!(Caret::word(text, 7), Caret::over(4..12));
    assert_eq!(Caret::word(text, 12), Caret::over(4..12));
    assert_eq!(Caret::word("a  b", 2), Caret::at(2));
}

#[test]
fn the_word_at_an_offset_includes_its_edges() {
    let text = "The customer waits.";
    assert_eq!(word_at(text, 7), Some(4..12));
    assert_eq!(word_at(text, 4), Some(4..12));
    assert_eq!(word_at(text, 12), Some(4..12));
    assert_eq!(word_at(text, 0), Some(0..3));
}

#[test]
fn spaces_and_punctuation_are_no_word() {
    assert_eq!(word_at("a  . b", 2), None);
    assert_eq!(word_at("a  . b", 4), None);
}

#[test]
fn word_moves_cross_blank_lines_and_word_at_finds_the_word_on_its_line() {
    let text = "one two\n\n\nthree, four\nfive";
    assert_eq!(Caret::at(13).moved(text, Motion::WordLeft).offset(), 10);
    assert_eq!(Caret::at(10).moved(text, Motion::WordLeft).offset(), 4);
    assert_eq!(Caret::at(7).moved(text, Motion::WordRight).offset(), 15);
    assert_eq!(Caret::at(21).moved(text, Motion::WordRight).offset(), 26);
    assert_eq!(Caret::at(26).moved(text, Motion::WordRight).offset(), 26);
    assert_eq!(Caret::at(2).moved(text, Motion::WordLeft).offset(), 0);
    assert_eq!(word_at(text, 10), Some(10..15));
    assert_eq!(word_at(text, 15), Some(10..15));
    assert_eq!(word_at(text, 16), None);
    assert_eq!(word_at(text, 9), None);
}

#[test]
fn a_flag_of_two_regional_indicators_is_one_grapheme_for_left_and_right() {
    let flags = "\u{1f1ec}\u{1f1f7}\u{1f1ec}\u{1f1f7}";
    assert_eq!(Caret::at(16).moved(flags, Motion::Left).offset(), 8);
    assert_eq!(Caret::at(0).moved(flags, Motion::Right).offset(), 8);
    assert_eq!(Caret::at(8).moved(flags, Motion::Right).offset(), 16);
}
