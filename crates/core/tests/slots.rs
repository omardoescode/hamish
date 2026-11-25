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
fn delete_the_active_variant_shows_the_original() {
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
    let mut document = Document::new("كاتب");
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
    assert_eq!(document.text(), "The cat sat.");
    document.set_active(paragraph, 1).unwrap();
    assert_eq!(document.text(), "Rain.");
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
