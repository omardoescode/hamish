use ka_teb_core::{Cycle, Document, Level, Provenance, SlotError};

fn variant_texts(document: &Document, slot: ka_teb_core::SlotId) -> Vec<String> {
    document.slot(slot).map_or_else(Vec::new, |slot| {
        slot.variants
            .iter()
            .map(|variant| variant.text().to_owned())
            .collect()
    })
}

#[test]
fn create_slot_at_each_level_keeps_the_range_text_as_the_original() {
    let text = "The cat sat. It purred.\n\nA second paragraph.";
    let examples = [
        (4..7, Level::Word, "cat"),
        (13..23, Level::Sentence, "It purred."),
        (25..44, Level::Paragraph, "A second paragraph."),
    ];
    for (range, level, original) in examples {
        let mut document = Document::new(text);
        let id = document
            .create_slot(range.clone(), level, "new text", Provenance::Writer)
            .unwrap();
        let slot = document.slot(id).unwrap();
        assert_eq!(slot.level, level);
        assert_eq!(slot.range, range);
        assert_eq!(slot.active, 0);
        assert_eq!(slot.variants[0].provenance(), Provenance::Writer);
        assert_eq!(slot.variants[1].provenance(), Provenance::Writer);
        assert_eq!(variant_texts(&document, id), [original, "new text"]);
        assert_eq!(document.text(), text);
    }
}

#[test]
fn set_active_variant_shows_it_in_the_text_and_counter() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "old dog", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    assert_eq!(document.text(), "The old dog sat.");
    assert_eq!(document.counter().to_string(), "4 words 16 chars");
    let slot = document.slot(id).unwrap();
    assert_eq!(slot.range, 4..11);
    assert_eq!(slot.active, 1);
    assert_eq!(variant_texts(&document, id), ["cat", "old dog"]);
}

#[test]
fn added_variants_keep_their_order_and_do_not_change_the_active_variant() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.add_variant(id, "fox", Provenance::Ai).unwrap();
    assert_eq!(variant_texts(&document, id), ["cat", "dog", "fox"]);
    assert_eq!(
        document.slot(id).unwrap().variants[2].provenance(),
        Provenance::Ai
    );
    assert_eq!(document.slot(id).unwrap().active, 0);
    assert_eq!(document.text(), "The cat sat.");
}

#[test]
fn cycle_moves_the_active_variant_and_wraps_around() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.add_variant(id, "fox", Provenance::Writer).unwrap();
    let steps = [
        (Cycle::Next, "The dog sat."),
        (Cycle::Next, "The fox sat."),
        (Cycle::Next, "The cat sat."),
        (Cycle::Previous, "The fox sat."),
    ];
    for (cycle, expected) in steps {
        document.cycle_active(id, cycle).unwrap();
        assert_eq!(document.text(), expected, "{cycle:?}");
    }
}

#[test]
fn slot_text_after_it_moves_with_an_active_variant_change() {
    let mut document = Document::new("A cat and a dog.");
    let cat = document
        .create_slot(2..5, Level::Word, "lion", Provenance::Writer)
        .unwrap();
    let dog = document
        .create_slot(12..15, Level::Word, "wolf", Provenance::Writer)
        .unwrap();
    document.set_active(cat, 1).unwrap();
    document.set_active(dog, 1).unwrap();
    assert_eq!(document.text(), "A lion and a wolf.");
    assert_eq!(document.slot(dog).unwrap().range, 13..17);
}

#[test]
fn operations_on_a_missing_slot_or_variant_are_refused() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    assert_eq!(
        document.set_active(id, 2),
        Err(SlotError::NoVariant { slot: id, index: 2 })
    );
    assert_eq!(document.text(), "The cat sat.");
}

#[test]
fn delete_an_inactive_variant_keeps_the_active_one() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.add_variant(id, "fox", Provenance::Writer).unwrap();
    document.set_active(id, 2).unwrap();
    document.delete_variant(id, 1).unwrap();
    assert_eq!(variant_texts(&document, id), ["cat", "fox"]);
    assert_eq!(document.slot(id).unwrap().active, 1);
    assert_eq!(document.text(), "The fox sat.");
}

#[test]
fn delete_the_active_variant_shows_the_next_one() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.add_variant(id, "fox", Provenance::Writer).unwrap();
    document.set_active(id, 1).unwrap();
    document.delete_variant(id, 1).unwrap();
    assert_eq!(variant_texts(&document, id), ["cat", "fox"]);
    assert_eq!(document.slot(id).unwrap().active, 1);
    assert_eq!(document.text(), "The fox sat.");
}

#[test]
fn undo_of_a_variant_delete_puts_it_back_at_its_position_as_active() {
    let mut document = Document::new("Oh, it takes over the room.");
    let id = document
        .create_slot(7..17, Level::Word, "commands attention", Provenance::Writer)
        .unwrap();
    document.add_variant(id, "reigns", Provenance::Ai).unwrap();
    document.set_active(id, 1).unwrap();
    document.delete_variant(id, 1).unwrap();
    assert_eq!(document.text(), "Oh, it reigns the room.");
    assert!(document.undo());
    assert_eq!(
        variant_texts(&document, id),
        ["takes over", "commands attention", "reigns"]
    );
    let slot = document.slot(id).unwrap();
    assert_eq!(slot.active, 1);
    assert_eq!(slot.range, 7..25);
    assert_eq!(document.text(), "Oh, it commands attention the room.");
}

#[test]
fn undo_of_the_delete_that_ended_a_slot_brings_the_slot_back() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.delete_variant(id, 0).unwrap();
    assert_eq!(document.slots(), Vec::new());
    assert!(document.undo());
    assert_eq!(variant_texts(&document, id), ["cat", "dog"]);
    assert_eq!(document.slot(id).unwrap().active, 0);
    assert_eq!(document.text(), "The cat sat.");
}

#[test]
fn slot_at_a_caret_is_the_innermost_slot_that_holds_it_edges_included() {
    let mut document = Document::new("The cat sat. It purred.");
    let sentence = document
        .create_slot(0..12, Level::Sentence, "A dog ran.", Provenance::Writer)
        .unwrap();
    let word = document
        .create_slot(4..7, Level::Word, "fox", Provenance::Writer)
        .unwrap();
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
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
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
fn delete_the_active_last_variant_wraps_to_the_original() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.add_variant(id, "fox", Provenance::Writer).unwrap();
    document.set_active(id, 2).unwrap();
    document.delete_variant(id, 2).unwrap();
    assert_eq!(document.slot(id).unwrap().active, 0);
    assert_eq!(document.text(), "The cat sat.");
}

#[test]
fn delete_the_active_original_shows_the_next_variant() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.add_variant(id, "fox", Provenance::Writer).unwrap();
    document.delete_variant(id, 0).unwrap();
    assert_eq!(variant_texts(&document, id), ["dog", "fox"]);
    assert_eq!(document.slot(id).unwrap().active, 0);
    assert_eq!(document.text(), "The dog sat.");
}

#[test]
fn slot_with_fewer_than_two_variants_ends_and_its_active_text_stays() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    document.delete_variant(id, 0).unwrap();
    assert_eq!(document.slot(id), None);
    assert_eq!(document.slots(), Vec::new());
    assert_eq!(document.text(), "The dog sat.");
}

#[test]
fn slot_ends_with_the_remaining_variant_shown_when_the_active_one_is_deleted() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.delete_variant(id, 0).unwrap();
    assert_eq!(document.slots(), Vec::new());
    assert_eq!(document.text(), "The dog sat.");
}

#[test]
fn edit_the_active_variant_updates_the_text_and_counter() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.edit_variant(id, 0, "big cat").unwrap();
    assert_eq!(document.text(), "The big cat sat.");
    assert_eq!(document.counter().to_string(), "4 words 16 chars");
    assert_eq!(variant_texts(&document, id), ["big cat", "dog"]);
    assert_eq!(document.slot(id).unwrap().range, 4..11);
}

#[test]
fn edit_an_inactive_variant_leaves_the_text() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.edit_variant(id, 1, "hound").unwrap();
    assert_eq!(variant_texts(&document, id), ["cat", "hound"]);
    assert_eq!(document.text(), "The cat sat.");
}

#[test]
fn writer_edit_makes_an_ai_variant_mixed_and_keeps_other_provenance() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Ai)
        .unwrap();
    document.add_variant(id, "fox", Provenance::Writer).unwrap();
    document.edit_variant(id, 1, "dogs").unwrap();
    document.edit_variant(id, 2, "foxes").unwrap();
    document.edit_variant(id, 1, "hounds").unwrap();
    let provenance: Vec<Provenance> = document
        .slot(id)
        .unwrap()
        .variants
        .iter()
        .map(ka_teb_core::Variant::provenance)
        .collect();
    assert_eq!(
        provenance,
        [Provenance::Writer, Provenance::Mixed, Provenance::Writer]
    );
}

#[test]
fn slot_that_partly_overlaps_another_is_refused() {
    let mut document = Document::new("The cat sat on the mat.");
    let id = document
        .create_slot(4..11, Level::Word, "dog ran", Provenance::Writer)
        .unwrap();
    assert_eq!(
        document.create_slot(8..14, Level::Word, "x", Provenance::Writer),
        Err(SlotError::PartialOverlap)
    );
    assert_eq!(
        document.create_slot(0..7, Level::Word, "x", Provenance::Writer),
        Err(SlotError::PartialOverlap)
    );
    let slots = document.slots();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].id, id);
}

#[test]
fn slot_over_an_empty_or_invalid_range_is_refused() {
    let mut document = Document::new("λέξη");
    assert_eq!(
        document.create_slot(2..2, Level::Word, "x", Provenance::Writer),
        Err(SlotError::EmptyRange)
    );
    assert!(matches!(
        document.create_slot(1..4, Level::Word, "x", Provenance::Writer),
        Err(SlotError::Range(_))
    ));
    assert_eq!(document.slots(), Vec::new());
}

#[test]
fn word_slot_inside_a_paragraph_variant_goes_away_and_back_with_it() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(0..12, Level::Paragraph, "A dog ran.", Provenance::Writer)
        .unwrap();
    let word = document
        .create_slot(4..7, Level::Word, "lion", Provenance::Writer)
        .unwrap();
    document.set_active(word, 1).unwrap();
    assert_eq!(document.text(), "The lion sat.");
    assert_eq!(document.slot(paragraph).unwrap().range, 0..13);
    assert_eq!(
        variant_texts(&document, paragraph),
        ["The lion sat.", "A dog ran."]
    );

    document.set_active(paragraph, 1).unwrap();
    assert_eq!(document.text(), "A dog ran.");
    assert_eq!(document.slot(word), None);

    document.set_active(paragraph, 0).unwrap();
    assert_eq!(document.text(), "The lion sat.");
    let word_slot = document.slot(word).unwrap();
    assert_eq!(word_slot.range, 4..8);
    assert_eq!(word_slot.active, 1);
}

#[test]
fn new_slot_around_an_existing_one_holds_it_in_its_original() {
    let mut document = Document::new("Intro. The cat sat.");
    let word = document
        .create_slot(11..14, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    let sentence = document
        .create_slot(7..19, Level::Sentence, "It slept.", Provenance::Writer)
        .unwrap();
    let ids: Vec<_> = document.slots().iter().map(|slot| slot.id).collect();
    assert_eq!(ids, [sentence, word]);
    document.set_active(sentence, 1).unwrap();
    assert_eq!(document.text(), "Intro. It slept.");
    assert_eq!(document.slot(word), None);
}

#[test]
fn slot_with_the_same_range_as_another_nests_inside_it() {
    let mut document = Document::new("The cat sat.");
    let sentence = document
        .create_slot(0..12, Level::Sentence, "A dog ran.", Provenance::Writer)
        .unwrap();
    let inner = document
        .create_slot(0..12, Level::Word, "Cats sit.", Provenance::Writer)
        .unwrap();
    document.set_active(inner, 1).unwrap();
    assert_eq!(document.text(), "Cats sit.");
    document.set_active(sentence, 1).unwrap();
    assert_eq!(document.slot(inner), None);
    assert_eq!(document.text(), "A dog ran.");
}

#[test]
fn deleting_a_variant_ends_the_slots_nested_in_it() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(0..12, Level::Paragraph, "A dog ran.", Provenance::Writer)
        .unwrap();
    document.set_active(paragraph, 1).unwrap();
    let word = document
        .create_slot(2..5, Level::Word, "fox", Provenance::Writer)
        .unwrap();
    document
        .add_variant(paragraph, "Rain.", Provenance::Writer)
        .unwrap();
    document.delete_variant(paragraph, 1).unwrap();
    assert_eq!(document.text(), "Rain.");
    document.set_active(paragraph, 0).unwrap();
    assert_eq!(document.text(), "The cat sat.");
    assert_eq!(document.slot(word), None);
    assert_eq!(document.slots().len(), 1);
}

#[test]
fn ending_a_slot_keeps_the_slots_nested_in_its_remaining_variant() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(0..12, Level::Paragraph, "A dog ran.", Provenance::Writer)
        .unwrap();
    let word = document
        .create_slot(4..7, Level::Word, "lion", Provenance::Writer)
        .unwrap();
    document.delete_variant(paragraph, 1).unwrap();
    assert_eq!(document.slot(paragraph), None);
    document.set_active(word, 1).unwrap();
    assert_eq!(document.text(), "The lion sat.");
}

#[test]
fn text_edit_before_a_slot_moves_it_and_after_leaves_it() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.insert(0, "See: ").unwrap();
    assert_eq!(document.slot(id).unwrap().range, 9..12);
    document.insert(16, " Yes.").unwrap();
    assert_eq!(document.slot(id).unwrap().range, 9..12);
    assert_eq!(variant_texts(&document, id), ["cat", "dog"]);
}

#[test]
fn insert_at_a_slot_edge_goes_outside_the_slot() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.insert(4, "big ").unwrap();
    document.insert(11, "s").unwrap();
    assert_eq!(document.text(), "The big cats sat.");
    assert_eq!(document.slot(id).unwrap().range, 8..11);
    assert_eq!(variant_texts(&document, id), ["cat", "dog"]);
}

#[test]
fn text_edit_inside_a_slot_changes_its_active_variant() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.replace(4..7, "kitten").unwrap();
    document.insert(5, "-").unwrap();
    assert_eq!(document.text(), "The k-itten sat.");
    assert_eq!(document.slot(id).unwrap().range, 4..11);
    assert_eq!(variant_texts(&document, id), ["k-itten", "dog"]);
}

#[test]
fn text_edit_over_part_of_a_slot_trims_it() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.replace(2..5, "ose b").unwrap();
    assert_eq!(document.text(), "Those bat sat.");
    assert_eq!(document.slot(id).unwrap().range, 7..9);
    assert_eq!(variant_texts(&document, id), ["at", "dog"]);
    document.delete(8..10).unwrap();
    assert_eq!(document.text(), "Those basat.");
    assert_eq!(variant_texts(&document, id), ["a", "dog"]);
}

#[test]
fn deleting_all_of_a_slot_and_more_ends_it() {
    let mut document = Document::new("The cat sat.");
    document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.delete(3..8).unwrap();
    assert_eq!(document.text(), "Thesat.");
    assert_eq!(document.slots(), Vec::new());
}

#[test]
fn writer_typing_in_an_active_ai_variant_makes_it_mixed() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Ai)
        .unwrap();
    document.set_active(id, 1).unwrap();
    assert_eq!(
        document.slot(id).unwrap().variants[1].provenance(),
        Provenance::Ai
    );
    document.insert(7, "s").unwrap();
    assert_eq!(
        document.slot(id).unwrap().variants[1].provenance(),
        Provenance::Ai
    );
    document.insert(5, "o").unwrap();
    assert_eq!(document.text(), "The doogs sat.");
    assert_eq!(
        document.slot(id).unwrap().variants[1].provenance(),
        Provenance::Mixed
    );
}

#[test]
fn typing_in_a_nested_slot_changes_both_active_variants() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(0..12, Level::Paragraph, "A dog ran.", Provenance::Writer)
        .unwrap();
    let word = document
        .create_slot(4..7, Level::Word, "lion", Provenance::Writer)
        .unwrap();
    document.insert(5, "h").unwrap();
    assert_eq!(variant_texts(&document, word), ["chat", "lion"]);
    assert_eq!(
        variant_texts(&document, paragraph),
        ["The chat sat.", "A dog ran."]
    );
    assert_eq!(document.slot(paragraph).unwrap().range, 0..13);
}

/// Active variant index and Variant texts of one visible Slot.
type SlotState<'a> = (usize, &'a [&'a str]);

fn state(document: &Document) -> (String, Vec<(usize, Vec<String>)>) {
    let slots = document
        .slots()
        .iter()
        .map(|slot| {
            let texts = slot.variants.iter().map(|v| v.text().to_owned()).collect();
            (slot.active, texts)
        })
        .collect();
    (document.text().to_owned(), slots)
}

fn expect_state(document: &Document, text: &str, slots: &[SlotState<'_>]) {
    let expected = slots
        .iter()
        .map(|(active, texts)| (*active, texts.iter().map(|t| (*t).to_owned()).collect()))
        .collect();
    assert_eq!(state(document), (text.to_owned(), expected));
}

#[test]
fn each_slot_operation_is_one_undoable_step() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.add_variant(id, "fox", Provenance::Ai).unwrap();
    document.set_active(id, 2).unwrap();
    document.edit_variant(id, 2, "foxes").unwrap();
    document.delete_variant(id, 0).unwrap();
    document.delete_variant(id, 0).unwrap();
    expect_state(&document, "The foxes sat.", &[]);

    let history: [(&str, &[SlotState<'_>]); 6] = [
        ("The foxes sat.", &[(1, &["dog", "foxes"])]),
        ("The foxes sat.", &[(2, &["cat", "dog", "foxes"])]),
        ("The fox sat.", &[(2, &["cat", "dog", "fox"])]),
        ("The cat sat.", &[(0, &["cat", "dog", "fox"])]),
        ("The cat sat.", &[(0, &["cat", "dog"])]),
        ("The cat sat.", &[]),
    ];
    for (text, slots) in history {
        assert!(document.undo());
        expect_state(&document, text, slots);
    }
    assert!(!document.undo());

    for _ in 0..6 {
        assert!(document.redo());
    }
    expect_state(&document, "The foxes sat.", &[]);
    assert_eq!(
        document.add_variant(id, "x", Provenance::Writer),
        Err(SlotError::NoSlot(id))
    );
}

#[test]
fn undo_of_a_text_edit_brings_back_the_slot_it_ended() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    document.delete(0..12).unwrap();
    assert!(document.undo());
    expect_state(&document, "The dog sat.", &[(1, &["cat", "dog"])]);
    assert_eq!(document.slot(id).unwrap().range, 4..7);
}

#[test]
fn refused_or_empty_operations_add_no_undo_step() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    assert!(
        document
            .create_slot(5..9, Level::Word, "x", Provenance::Writer)
            .is_err()
    );
    assert!(document.set_active(id, 5).is_err());
    document.set_active(id, 0).unwrap();
    document.edit_variant(id, 1, "dog").unwrap();
    assert!(document.undo());
    expect_state(&document, "The cat sat.", &[]);
    assert!(!document.undo());
}

#[test]
fn variant_switch_in_a_nested_slot_keeps_the_outer_ai_variant_ai() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(0..12, Level::Paragraph, "A dog ran.", Provenance::Ai)
        .unwrap();
    document.set_active(paragraph, 1).unwrap();
    let word = document
        .create_slot(2..5, Level::Word, "wolf", Provenance::Writer)
        .unwrap();
    document.set_active(word, 1).unwrap();
    document.cycle_active(word, Cycle::Next).unwrap();
    document.set_active(word, 1).unwrap();
    assert_eq!(document.text(), "A wolf ran.");
    assert_eq!(
        variant_texts(&document, paragraph),
        ["The cat sat.", "A wolf ran."]
    );
    assert_eq!(
        document.slot(paragraph).unwrap().variants[1].provenance(),
        Provenance::Ai
    );
}

#[test]
fn writer_edit_of_a_nested_active_variant_makes_the_outer_ai_variant_mixed() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(0..12, Level::Paragraph, "A dog ran.", Provenance::Ai)
        .unwrap();
    document.set_active(paragraph, 1).unwrap();
    let word = document
        .create_slot(2..5, Level::Word, "wolf", Provenance::Writer)
        .unwrap();
    document.edit_variant(word, 1, "fox").unwrap();
    assert_eq!(
        document.slot(paragraph).unwrap().variants[1].provenance(),
        Provenance::Ai
    );
    document.edit_variant(word, 0, "pup").unwrap();
    assert_eq!(document.text(), "A pup ran.");
    assert_eq!(
        document.slot(paragraph).unwrap().variants[1].provenance(),
        Provenance::Mixed
    );
}

#[test]
fn each_paragraph_variant_keeps_its_own_inner_slots_and_choices() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(
            0..12,
            Level::Paragraph,
            "A dog ran. It barked.",
            Provenance::Writer,
        )
        .unwrap();
    let cat = document
        .create_slot(4..7, Level::Word, "lion", Provenance::Writer)
        .unwrap();
    document.set_active(cat, 1).unwrap();
    document.set_active(paragraph, 1).unwrap();
    let dog = document
        .create_slot(2..5, Level::Word, "wolf", Provenance::Writer)
        .unwrap();
    let barked = document
        .create_slot(
            11..21,
            Level::Sentence,
            "It howled at the moon.",
            Provenance::Writer,
        )
        .unwrap();
    document.set_active(barked, 1).unwrap();
    assert_eq!(document.text(), "A dog ran. It howled at the moon.");
    assert_eq!(document.counter().to_string(), "8 words 33 chars");
    let ids: Vec<_> = document.slots().iter().map(|slot| slot.id).collect();
    assert_eq!(ids, [paragraph, dog, barked]);

    document.set_active(paragraph, 0).unwrap();
    assert_eq!(document.text(), "The lion sat.");
    assert_eq!(document.counter().to_string(), "3 words 13 chars");
    let ids: Vec<_> = document.slots().iter().map(|slot| slot.id).collect();
    assert_eq!(ids, [paragraph, cat]);

    document.set_active(paragraph, 1).unwrap();
    assert_eq!(document.text(), "A dog ran. It howled at the moon.");
    assert_eq!(document.slot(dog).unwrap().active, 0);
    assert_eq!(document.slot(barked).unwrap().range, 11..33);
    assert_eq!(document.slot(barked).unwrap().active, 1);
}

#[test]
fn nested_slot_with_an_empty_variant_at_the_paragraph_edge_stays_inside_it() {
    let mut document = Document::new("Intro.\nVery old cats sit\nEnd.");
    let paragraph = document
        .create_slot(7..24, Level::Paragraph, "Dogs run", Provenance::Writer)
        .unwrap();
    let first = document
        .create_slot(7..12, Level::Word, "", Provenance::Writer)
        .unwrap();
    let last = document
        .create_slot(21..24, Level::Word, "", Provenance::Writer)
        .unwrap();
    document.set_active(first, 1).unwrap();
    document.set_active(last, 1).unwrap();
    assert_eq!(document.text(), "Intro.\nold cats \nEnd.");
    document.set_active(first, 0).unwrap();
    document.set_active(last, 0).unwrap();
    assert_eq!(document.text(), "Intro.\nVery old cats sit\nEnd.");
    assert_eq!(document.slot(paragraph).unwrap().range, 7..24);
    assert_eq!(
        variant_texts(&document, paragraph),
        ["Very old cats sit", "Dogs run"]
    );
    document.set_active(paragraph, 1).unwrap();
    assert_eq!(document.text(), "Intro.\nDogs run\nEnd.");
}

#[test]
fn partial_overlap_is_refused_inside_a_paragraph_variant_and_across_its_edge() {
    let mut document = Document::new("Intro. The cat sat on the mat. End.");
    let paragraph = document
        .create_slot(
            7..30,
            Level::Paragraph,
            "A dog ran far.",
            Provenance::Writer,
        )
        .unwrap();
    document.set_active(paragraph, 1).unwrap();
    let inner = document
        .create_slot(9..16, Level::Word, "fox sped", Provenance::Writer)
        .unwrap();
    let refused = [3..10, 12..19, 18..24];
    for range in refused {
        assert_eq!(
            document.create_slot(range.clone(), Level::Word, "x", Provenance::Writer),
            Err(SlotError::PartialOverlap),
            "{range:?}"
        );
    }
    let ids: Vec<_> = document.slots().iter().map(|slot| slot.id).collect();
    assert_eq!(ids, [paragraph, inner]);
    assert_eq!(document.text(), "Intro. A dog ran far. End.");
}

#[test]
fn undo_restores_nested_slots_and_their_choices() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(0..12, Level::Paragraph, "A dog ran.", Provenance::Writer)
        .unwrap();
    document.set_active(paragraph, 1).unwrap();
    let dog = document
        .create_slot(2..5, Level::Word, "wolf", Provenance::Writer)
        .unwrap();
    document.set_active(dog, 1).unwrap();
    document.set_active(paragraph, 0).unwrap();
    expect_state(
        &document,
        "The cat sat.",
        &[(0, &["The cat sat.", "A wolf ran."])],
    );

    let history: [(&str, &[SlotState<'_>]); 3] = [
        (
            "A wolf ran.",
            &[(1, &["The cat sat.", "A wolf ran."]), (1, &["dog", "wolf"])],
        ),
        (
            "A dog ran.",
            &[(1, &["The cat sat.", "A dog ran."]), (0, &["dog", "wolf"])],
        ),
        ("A dog ran.", &[(1, &["The cat sat.", "A dog ran."])]),
    ];
    for (text, slots) in history {
        assert!(document.undo());
        expect_state(&document, text, slots);
    }
    assert!(document.redo());
    assert!(document.redo());
    assert!(document.redo());
    expect_state(
        &document,
        "The cat sat.",
        &[(0, &["The cat sat.", "A wolf ran."])],
    );
    document.set_active(paragraph, 1).unwrap();
    assert_eq!(document.slot(dog).unwrap().active, 1);
}
