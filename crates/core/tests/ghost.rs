#![expect(
    clippy::single_range_in_vec_init,
    reason = "a list of one Ghost range is the expected value"
)]

use hamish_core::{Document, GhostError, Level, Provenance, SlotError};

#[test]
fn ghost_keeps_the_text_and_marks_the_range() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    assert_eq!(document.text(), "Keep this aside now.");
    assert_eq!(document.ghost_ranges(), [5..15]);
}

#[test]
fn revive_returns_the_whole_ghost_range_to_normal() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    document.revive(7..7).unwrap();
    assert_eq!(document.ghost_ranges(), []);
    assert_eq!(document.text(), "Keep this aside now.");
}

#[test]
fn revive_outside_a_ghost_range_is_refused() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    assert_eq!(document.revive(16..19), Err(GhostError::NoGhost(16..19)));
    assert_eq!(document.ghost_ranges(), [5..15]);
}

#[test]
fn ghost_of_an_empty_or_invalid_range_is_refused() {
    let mut document = Document::new("Keep this.");
    assert_eq!(document.ghost(3..3), Err(GhostError::EmptyRange));
    assert!(matches!(document.ghost(4..40), Err(GhostError::Range(_))));
    assert_eq!(document.ghost_ranges(), []);
    assert!(!document.undo());
}

#[test]
fn overlapping_and_touching_ghost_ranges_join() {
    let mut document = Document::new("one two three four five");
    document.ghost(4..7).unwrap();
    document.ghost(6..13).unwrap();
    document.ghost(13..18).unwrap();
    assert_eq!(document.ghost_ranges(), [4..18]);
}

#[test]
fn ghost_and_revive_are_undoable() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    document.revive(5..15).unwrap();
    assert!(document.undo());
    assert_eq!(document.ghost_ranges(), [5..15]);
    assert!(document.undo());
    assert_eq!(document.ghost_ranges(), []);
    assert!(document.redo());
    assert_eq!(document.ghost_ranges(), [5..15]);
}

#[test]
fn counter_excludes_ghost_text() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    assert_eq!(document.counter().to_string(), "2 words 10 chars");
}

#[test]
fn a_ghost_boundary_splits_words_for_the_counter() {
    let mut document = Document::new("abcd");
    document.ghost(1..3).unwrap();
    assert_eq!(document.counter().to_string(), "2 words 2 chars");
}

#[test]
fn editing_inside_a_ghost_range_keeps_it_ghost() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    document.replace(10..15, "away").unwrap();
    assert_eq!(document.text(), "Keep this away now.");
    assert_eq!(document.ghost_ranges(), [5..14]);
    document.replace(5..14, "it").unwrap();
    assert_eq!(document.text(), "Keep it now.");
    assert_eq!(document.ghost_ranges(), [5..7]);
}

#[test]
fn ghost_ranges_move_with_text_edits_around_them() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    document.insert(0, "Do ").unwrap();
    assert_eq!(document.ghost_ranges(), [8..18]);
    document.insert(18, " right").unwrap();
    assert_eq!(document.text(), "Do Keep this aside right now.");
    assert_eq!(document.ghost_ranges(), [8..18]);
    document.insert(8, "all ").unwrap();
    assert_eq!(document.ghost_ranges(), [12..22]);
}

#[test]
fn deleting_part_of_a_ghost_range_trims_it_and_deleting_all_of_it_ends_it() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    document.delete(0..7).unwrap();
    assert_eq!(document.text(), "is aside now.");
    assert_eq!(document.ghost_ranges(), [0..8]);
    document.delete(0..9).unwrap();
    assert_eq!(document.text(), "now.");
    assert_eq!(document.ghost_ranges(), []);
    assert!(document.undo());
    assert_eq!(document.ghost_ranges(), [0..8]);
}

#[test]
fn selection_counter_excludes_ghost_text() {
    let mut document = Document::new("Keep this aside now.");
    document.ghost(5..15).unwrap();
    assert_eq!(
        document.selection_counter(0..12).unwrap().to_string(),
        "1 word 5 chars"
    );
    assert_eq!(
        document.selection_counter(7..20).unwrap().to_string(),
        "1 word 5 chars"
    );
}

#[test]
fn ghost_that_partly_overlaps_a_slot_is_refused() {
    let mut document = Document::new("The cat sat on the mat.");
    document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    assert_eq!(document.ghost(2..6), Err(GhostError::PartialOverlap));
    assert_eq!(document.ghost(5..11), Err(GhostError::PartialOverlap));
    assert_eq!(document.ghost_ranges(), []);
}

#[test]
fn slot_that_partly_overlaps_a_ghost_range_is_refused() {
    let mut document = Document::new("The cat sat on the mat.");
    document.ghost(8..14).unwrap();
    assert_eq!(
        document.create_slot(4..10, Level::Word, "dog", Provenance::Writer),
        Err(SlotError::PartialOverlap)
    );
    assert_eq!(document.slots(), []);
}

#[test]
fn ghost_inside_a_slot_shows_only_with_its_variant() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "old dog", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    document.ghost(4..7).unwrap();
    assert_eq!(document.ghost_ranges(), [4..7]);
    assert_eq!(document.counter().to_string(), "3 words 13 chars");
    document.set_active(id, 0).unwrap();
    assert_eq!(document.text(), "The cat sat.");
    assert_eq!(document.ghost_ranges(), []);
    document.set_active(id, 1).unwrap();
    assert_eq!(document.ghost_ranges(), [4..7]);
}

#[test]
fn ghost_that_holds_a_slot_stays_whichever_variant_shows() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "old dog", Provenance::Writer)
        .unwrap();
    document.ghost(4..7).unwrap();
    document.set_active(id, 1).unwrap();
    assert_eq!(document.ghost_ranges(), [4..11]);
    document.revive(5..5).unwrap();
    document.ghost(0..12).unwrap();
    document.set_active(id, 0).unwrap();
    assert_eq!(document.ghost_ranges(), [0..8]);
}

#[test]
fn slot_over_a_ghost_range_keeps_it_in_the_original() {
    let mut document = Document::new("The cat sat on the mat.");
    document.ghost(4..7).unwrap();
    let id = document
        .create_slot(0..11, Level::Sentence, "A dog ran", Provenance::Writer)
        .unwrap();
    assert_eq!(document.ghost_ranges(), [4..7]);
    document.set_active(id, 1).unwrap();
    assert_eq!(document.text(), "A dog ran on the mat.");
    assert_eq!(document.ghost_ranges(), []);
    document.set_active(id, 0).unwrap();
    assert_eq!(document.ghost_ranges(), [4..7]);
}

#[test]
fn revive_reaches_ghost_ranges_nested_in_slots() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "old dog", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    document.ghost(8..11).unwrap();
    document.ghost(0..12).unwrap();
    document.revive(9..9).unwrap();
    assert_eq!(document.ghost_ranges(), []);
}

#[test]
fn ghost_nested_in_a_slot_stays_when_the_slot_ends() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "old dog", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    document.ghost(8..11).unwrap();
    document.delete_variant(id, 0).unwrap();
    assert_eq!(document.slots(), []);
    assert_eq!(document.text(), "The old dog sat.");
    assert_eq!(document.ghost_ranges(), [8..11]);
}

#[test]
fn editing_a_variant_ends_the_ghost_ranges_in_it() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "old dog", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    document.ghost(4..7).unwrap();
    document.edit_variant(id, 1, "red fox").unwrap();
    assert_eq!(document.text(), "The red fox sat.");
    assert_eq!(document.ghost_ranges(), []);
    assert!(document.undo());
    assert_eq!(document.ghost_ranges(), [4..7]);
}

#[test]
fn a_caret_on_the_edge_of_a_ghost_touches_it_and_a_range_beside_it_does_not() {
    let mut document = Document::new("Keep this aside now, then go on.");
    document.ghost(4..12).unwrap();
    document.ghost(20..25).unwrap();
    assert!(document.touches_ghost(&(12..12)));
    assert!(document.touches_ghost(&(10..14)));
    assert!(!document.touches_ghost(&(12..14)));
    assert!(!document.touches_ghost(&(0..3)));
    assert!(document.touches_ghost(&(24..30)));
}

#[test]
fn toggle_ghost_ghosts_a_range_and_revives_one_it_touches() {
    let mut document = Document::new("Keep this aside now.");
    document.toggle_ghost(5..9).unwrap();
    assert_eq!(document.ghost_ranges(), [5..9]);

    document.toggle_ghost(7..7).unwrap();
    assert_eq!(document.ghost_ranges(), []);
}
