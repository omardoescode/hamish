mod fixtures {
    pub mod slot;
    pub mod state;
}

use fixtures::slot::writer_slot;
use fixtures::state::{SlotState, expect_state, variant_texts};
use ka_teb_core::{Cycle, Document, Level, Provenance, SlotError};

#[test]
fn word_slot_inside_a_paragraph_variant_goes_away_and_back_with_it() {
    let mut document = Document::new("The cat sat.");
    let paragraph = writer_slot(&mut document, 0..12, Level::Paragraph, "A dog ran.");
    let word = writer_slot(&mut document, 4..7, Level::Word, "lion");
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
    let word = writer_slot(&mut document, 11..14, Level::Word, "dog");
    let sentence = writer_slot(&mut document, 7..19, Level::Sentence, "It slept.");
    let ids: Vec<_> = document.slots().iter().map(|slot| slot.id).collect();
    assert_eq!(ids, [sentence, word]);
    document.set_active(sentence, 1).unwrap();
    assert_eq!(document.text(), "Intro. It slept.");
    assert_eq!(document.slot(word), None);
}

#[test]
fn slot_with_the_same_range_as_another_nests_inside_it() {
    let mut document = Document::new("The cat sat.");
    let sentence = writer_slot(&mut document, 0..12, Level::Sentence, "A dog ran.");
    let inner = writer_slot(&mut document, 0..12, Level::Word, "Cats sit.");
    document.set_active(inner, 1).unwrap();
    assert_eq!(document.text(), "Cats sit.");
    document.set_active(sentence, 1).unwrap();
    assert_eq!(document.slot(inner), None);
    assert_eq!(document.text(), "A dog ran.");
}

#[test]
fn deleting_a_variant_ends_the_slots_nested_in_it() {
    let mut document = Document::new("The cat sat.");
    let paragraph = writer_slot(&mut document, 0..12, Level::Paragraph, "A dog ran.");
    document.set_active(paragraph, 1).unwrap();
    let word = writer_slot(&mut document, 2..5, Level::Word, "fox");
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
    let paragraph = writer_slot(&mut document, 0..12, Level::Paragraph, "A dog ran.");
    let word = writer_slot(&mut document, 4..7, Level::Word, "lion");
    document.delete_variant(paragraph, 1).unwrap();
    assert_eq!(document.slot(paragraph), None);
    document.set_active(word, 1).unwrap();
    assert_eq!(document.text(), "The lion sat.");
}

#[test]
fn typing_in_a_nested_slot_changes_both_active_variants() {
    let mut document = Document::new("The cat sat.");
    let paragraph = writer_slot(&mut document, 0..12, Level::Paragraph, "A dog ran.");
    let word = writer_slot(&mut document, 4..7, Level::Word, "lion");
    document.insert(5, "h").unwrap();
    assert_eq!(variant_texts(&document, word), ["chat", "lion"]);
    assert_eq!(
        variant_texts(&document, paragraph),
        ["The chat sat.", "A dog ran."]
    );
    assert_eq!(document.slot(paragraph).unwrap().range, 0..13);
}

#[test]
fn variant_switch_in_a_nested_slot_keeps_the_outer_ai_variant_ai() {
    let mut document = Document::new("The cat sat.");
    let paragraph = document
        .create_slot(0..12, Level::Paragraph, "A dog ran.", Provenance::Ai)
        .unwrap();
    document.set_active(paragraph, 1).unwrap();
    let word = writer_slot(&mut document, 2..5, Level::Word, "wolf");
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
    let word = writer_slot(&mut document, 2..5, Level::Word, "wolf");
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
    let paragraph = writer_slot(
        &mut document,
        0..12,
        Level::Paragraph,
        "A dog ran. It barked.",
    );
    let cat = writer_slot(&mut document, 4..7, Level::Word, "lion");
    document.set_active(cat, 1).unwrap();
    document.set_active(paragraph, 1).unwrap();
    let dog = writer_slot(&mut document, 2..5, Level::Word, "wolf");
    let barked = writer_slot(
        &mut document,
        11..21,
        Level::Sentence,
        "It howled at the moon.",
    );
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
    let paragraph = writer_slot(&mut document, 7..24, Level::Paragraph, "Dogs run");
    let first = writer_slot(&mut document, 7..12, Level::Word, "");
    let last = writer_slot(&mut document, 21..24, Level::Word, "");
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
    let paragraph = writer_slot(&mut document, 7..30, Level::Paragraph, "A dog ran far.");
    document.set_active(paragraph, 1).unwrap();
    let inner = writer_slot(&mut document, 9..16, Level::Word, "fox sped");
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
    let paragraph = writer_slot(&mut document, 0..12, Level::Paragraph, "A dog ran.");
    document.set_active(paragraph, 1).unwrap();
    let dog = writer_slot(&mut document, 2..5, Level::Word, "wolf");
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

#[test]
fn the_kept_text_range_of_an_ended_nested_slot_is_in_the_document() {
    let mut document = Document::new(
        "Title
The cat sat.",
    );
    let sentence = writer_slot(&mut document, 6..18, Level::Sentence, "No.");
    let word = writer_slot(&mut document, 10..13, Level::Word, "dog");
    assert_eq!(document.delete_variant(word, 0), Ok(Some(10..13)));
    assert_eq!(
        document.text(),
        "Title
The dog sat."
    );
    assert!(document.slot(sentence).is_some());
}
