use hamish_core::{Document, Level, OverflowError, Provenance};

#[test]
fn overflow_text_can_be_typed_pasted_and_edited() {
    let mut document = Document::new("Body.");
    document.edit_overflow(0..0, "a note").unwrap();
    document
        .edit_overflow(6..6, "\nhttps://example.org")
        .unwrap();
    document.edit_overflow(2..6, "link").unwrap();
    assert_eq!(document.overflow().text(), "a link\nhttps://example.org");
    assert_eq!(document.text(), "Body.");
}

#[test]
fn overflow_edits_are_undoable() {
    let mut document = Document::new("Body.");
    document.edit_overflow(0..0, "note").unwrap();
    document.insert(5, " More.").unwrap();
    assert!(document.undo());
    assert!(document.undo());
    assert_eq!(document.overflow().text(), "");
    assert_eq!(document.text(), "Body.");
    assert!(document.redo());
    assert_eq!(document.overflow().text(), "note");
}

#[test]
fn overflow_edit_of_an_invalid_range_is_refused() {
    let mut document = Document::new("Body.");
    document.edit_overflow(0..0, "note").unwrap();
    assert!(document.edit_overflow(2..9, "x").is_err());
    assert_eq!(document.overflow().text(), "note");
}

#[test]
fn stash_moves_the_selection_to_the_end_of_overflow() {
    let mut document = Document::new("Keep this. Cut this. Keep that.");
    document.stash(11..21).unwrap();
    assert_eq!(document.text(), "Keep this. Keep that.");
    assert_eq!(document.overflow().text(), "Cut this. ");
}

#[test]
fn stash_starts_a_new_block_after_existing_overflow_text() {
    let mut document = Document::new("One. Two. Three.");
    document.edit_overflow(0..0, "a note").unwrap();
    document.stash(5..9).unwrap();
    document.edit_overflow(12..12, "\n\n").unwrap();
    document.stash(5..12).unwrap();
    assert_eq!(document.overflow().text(), "a note\n\nTwo.\n\n Three.");
    assert_eq!(document.text(), "One. ");
}

#[test]
fn stash_is_one_undo_step() {
    let mut document = Document::new("Keep this. Cut this.");
    document.stash(10..20).unwrap();
    assert!(document.undo());
    assert_eq!(document.text(), "Keep this. Cut this.");
    assert_eq!(document.overflow().text(), "");
    assert!(document.redo());
    assert_eq!(document.text(), "Keep this.");
    assert_eq!(document.overflow().text(), " Cut this.");
}

#[test]
fn stash_after_a_blank_line_adds_no_second_one() {
    let mut document = Document::new("One. Two.");
    document.edit_overflow(0..0, "a note\n").unwrap();
    document.stash(5..9).unwrap();
    assert_eq!(document.overflow().text(), "a note\n\nTwo.");
}

#[test]
fn stash_of_an_empty_or_invalid_range_is_refused() {
    let mut document = Document::new("Body.");
    assert_eq!(document.stash(2..2), Err(OverflowError::EmptyRange));
    assert!(matches!(document.stash(2..9), Err(OverflowError::Range(_))));
    assert_eq!(document.text(), "Body.");
    assert!(!document.undo());
}

#[test]
fn stash_of_a_range_that_holds_a_slot_moves_its_active_text_and_undo_brings_the_slot_back() {
    let mut document = Document::new("Keep this. A big cat. End.");
    let id = document
        .create_slot(13..16, Level::Word, "huge", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    document.stash(11..23).unwrap();
    assert_eq!(document.text(), "Keep this. End.");
    assert_eq!(document.overflow().text(), "A huge cat. ");
    assert_eq!(document.slots().len(), 0);
    assert!(document.undo());
    assert_eq!(document.slot(id).map(|slot| slot.range), Some(13..17));
}

#[test]
fn blank_lines_split_overflow_into_blocks() {
    let mut document = Document::new("Body.");
    document
        .edit_overflow(0..0, "\n first\nstill first\n\n \n\nsecond\n")
        .unwrap();
    assert_eq!(document.overflow().blocks(), [1..19, 24..30]);
}

#[test]
fn use_block_inserts_the_block_text_and_keeps_it_in_overflow() {
    let mut document = Document::new("Start. End.");
    document
        .edit_overflow(0..0, "a note\n\nMiddle line one.\nMiddle line two. \n")
        .unwrap();
    document.use_block(20, 7).unwrap();
    assert_eq!(
        document.text(),
        "Start. Middle line one.\nMiddle line two. End."
    );
    assert_eq!(
        document.overflow().text(),
        "a note\n\nMiddle line one.\nMiddle line two. \n"
    );
}

#[test]
fn use_block_takes_the_block_the_caret_is_in_or_at_the_end_of() {
    let mut document = Document::new("Body.");
    document.edit_overflow(0..0, "one\n\ntwo").unwrap();
    document.use_block(3, 0).unwrap();
    document.use_block(5, 0).unwrap();
    assert_eq!(document.text(), "two one Body.");
}

#[test]
fn use_block_outside_a_block_is_refused() {
    let mut document = Document::new("Body.");
    document.edit_overflow(0..0, "one\n\n\ntwo").unwrap();
    assert_eq!(document.use_block(4, 0), Err(OverflowError::NoBlock(4)));
    assert!(matches!(
        document.use_block(0, 9),
        Err(OverflowError::Range(_))
    ));
    assert_eq!(document.text(), "Body.");
}

/// "the light changed over it." with Overflow "paragraph".
fn dropping() -> Document {
    let mut document = Document::new("the light changed over it.");
    assert!(
        document.edit_overflow(0..0, "paragraph").is_ok(),
        "valid range"
    );
    document
}

#[test]
fn use_block_between_words_puts_a_space_on_each_side() {
    let mut after_word = dropping();
    after_word.use_block(0, 17).unwrap();
    assert_eq!(after_word.text(), "the light changed paragraph over it.");

    let mut before_word = dropping();
    before_word.use_block(0, 4).unwrap();
    assert_eq!(before_word.text(), "the paragraph light changed over it.");
}

#[test]
fn use_block_puts_no_space_before_punctuation_or_next_to_a_space() {
    let mut before_stop = dropping();
    before_stop.use_block(0, 25).unwrap();
    assert_eq!(before_stop.text(), "the light changed over it paragraph.");

    let mut at_start = dropping();
    at_start.use_block(0, 0).unwrap();
    assert_eq!(at_start.text(), "paragraph the light changed over it.");

    let mut at_end = dropping();
    at_end.use_block(0, 26).unwrap();
    assert_eq!(at_end.text(), "the light changed over it. paragraph");

    let mut spaced = Document::new("one two");
    spaced.edit_overflow(0..0, "block ").unwrap();
    spaced.use_block(0, 4).unwrap();
    assert_eq!(spaced.text(), "one block two");
}

#[test]
fn use_block_spaced_is_one_undo_step() {
    let mut document = dropping();
    document.use_block(0, 17).unwrap();
    assert!(document.undo());
    assert_eq!(document.text(), "the light changed over it.");
}

#[test]
fn use_block_is_one_undo_step() {
    let mut document = Document::new("Body.");
    document.edit_overflow(0..0, "Lead. ").unwrap();
    document.use_block(0, 0).unwrap();
    assert!(document.undo());
    assert_eq!(document.text(), "Body.");
    assert_eq!(document.overflow().text(), "Lead. ");
    assert!(document.redo());
    assert_eq!(document.text(), "Lead. Body.");
}
