use ka_teb_core::{Document, Level, OverflowError, Provenance};

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
