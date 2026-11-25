mod fixtures {
    pub mod slot;
}

use fixtures::slot::writer_slot;
use ka_teb_core::{Document, Level, Provenance};

#[test]
fn slot_at_a_caret_is_the_innermost_slot_that_holds_it_edges_included() {
    let mut document = Document::new("The cat sat. It purred.");
    let sentence = writer_slot(&mut document, 0..12, Level::Sentence, "A dog ran.");
    let word = writer_slot(&mut document, 4..7, Level::Word, "fox");
    let examples = [
        (0, Some(sentence)),
        (3, Some(sentence)),
        (4, Some(word)),
        (5, Some(word)),
        (7, Some(word)),
        (8, Some(sentence)),
        (12, Some(sentence)),
        (13, None),
        (23, None),
    ];
    for (caret, expected) in examples {
        assert_eq!(document.slot_at(caret), expected, "caret {caret}");
    }
}

#[test]
fn undo_restores_variant_names_the_slot_when_the_last_step_is_a_variant_delete() {
    let mut document = Document::new("The cat sat.");
    let id = writer_slot(&mut document, 4..7, Level::Word, "dog");
    document.add_variant(id, "fox", Provenance::Writer).unwrap();
    assert_eq!(document.undo_restores_variant(), None);
    document.delete_variant(id, 1).unwrap();
    assert_eq!(document.undo_restores_variant(), Some(id));
    document.insert(0, "So ").unwrap();
    assert_eq!(document.undo_restores_variant(), None);
    assert!(document.undo());
    assert_eq!(document.undo_restores_variant(), Some(id));
    document.delete_variant(id, 0).unwrap();
    assert_eq!(document.slots(), Vec::new());
    assert_eq!(document.undo_restores_variant(), Some(id));
    assert!(document.undo());
    assert!(document.undo());
    assert_eq!(document.undo_restores_variant(), None);
}

#[test]
fn a_new_slot_over_a_word_or_short_phrase_is_word_level() {
    let document = Document::new("Title\nThe customer waits. She leaves.\nNext one.");
    assert_eq!(document.level_for(&(10..18)), Level::Word);
    assert_eq!(document.level_for(&(6..18)), Level::Word);
}

#[test]
fn a_new_slot_over_text_with_sentence_end_punctuation_is_sentence_level() {
    let document = Document::new("Title\nThe customer waits. She leaves.\nNext one.");
    assert_eq!(document.level_for(&(6..25)), Level::Sentence);
}

#[test]
fn a_new_slot_over_a_whole_paragraph_or_across_lines_is_paragraph_level() {
    let document = Document::new("Title\nThe customer waits. She leaves.\nNext one.");
    assert_eq!(document.level_for(&(6..37)), Level::Paragraph);
    assert_eq!(document.level_for(&(26..42)), Level::Paragraph);
}

#[test]
fn slot_over_a_range_is_the_innermost_slot_that_holds_all_of_it() {
    let mut document = Document::new("The cat sat. It purred.");
    let sentence = writer_slot(&mut document, 0..12, Level::Sentence, "A dog ran.");
    let word = writer_slot(&mut document, 4..7, Level::Word, "fox");
    let examples = [
        (4..7, Some(word)),
        (5..6, Some(word)),
        (4..8, Some(sentence)),
        (0..12, Some(sentence)),
        (10..14, None),
        (13..23, None),
    ];
    for (range, expected) in examples {
        assert_eq!(document.slot_over(&range), expected, "range {range:?}");
    }
}
