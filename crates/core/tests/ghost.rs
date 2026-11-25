#![expect(
    clippy::single_range_in_vec_init,
    reason = "a list of one Ghost range is the expected value"
)]

use ka_teb_core::{Document, GhostError};

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
