#![expect(
    clippy::single_range_in_vec_init,
    reason = "a Tree holds Vecs of byte ranges; one range is one Slot or Ghost range"
)]

use super::*;
use crate::{Level, Provenance, Variant};

fn node(id: u64, range: Range<usize>, original: &str, other: &str) -> SlotNode {
    SlotNode {
        id: SlotId(id),
        level: Level::Word,
        range,
        variants: vec![
            Variant::new(original, Provenance::Writer),
            Variant::new(other, Provenance::Ai),
        ],
        active: 0,
    }
}

fn ranges(tree: &Tree) -> Vec<Range<usize>> {
    tree.visible_slots()
        .into_iter()
        .map(|slot| slot.range)
        .collect()
}

#[test]
fn place_nests_a_slot_in_the_active_variant_of_the_slot_that_holds_it() {
    let mut tree = Tree::default();
    tree.place(node(1, 0..8, "aa bb cc", "dd")).unwrap();
    tree.place(node(2, 3..5, "bb", "ee")).unwrap();

    assert_eq!(tree.slots().len(), 1);
    assert_eq!(ranges(&tree), [0..8, 3..5]);
    assert_eq!(tree.slots()[0].variants[0].tree.slots()[0].range, 3..5);
}

#[test]
fn place_moves_the_slots_and_ghost_ranges_inside_a_new_slot_into_its_original() {
    let mut tree = Tree::default();
    tree.place(node(1, 3..5, "bb", "ee")).unwrap();
    tree.place_ghost(6..8).unwrap();
    tree.place(node(2, 0..8, "aa bb cc", "dd")).unwrap();

    assert_eq!(tree.ghosts(), []);
    assert_eq!(tree.ghost_ranges(), [6..8]);
    assert_eq!(tree.slots()[0].variants[0].tree.ghosts(), [6..8]);
    assert_eq!(ranges(&tree), [0..8, 3..5]);
}

#[test]
fn place_refuses_a_partial_overlap_and_changes_nothing() {
    let mut tree = Tree::default();
    tree.place(node(1, 0..5, "aa bb", "dd")).unwrap();
    let before = tree.clone();

    assert_eq!(
        tree.place(node(2, 3..8, "bb cc", "ee")),
        Err(SlotError::PartialOverlap)
    );
    assert_eq!(tree, before);
}

#[test]
fn a_ghost_range_equal_to_a_slot_stays_outside_it() {
    let mut tree = Tree::default();
    tree.place(node(1, 0..5, "aa bb", "dd")).unwrap();
    tree.place_ghost(0..5).unwrap();

    assert_eq!(tree.ghosts(), [0..5]);
}

#[test]
fn place_ghost_refuses_a_partial_overlap_with_a_slot() {
    let mut tree = Tree::default();
    tree.place(node(1, 0..5, "aa bb", "dd")).unwrap();

    assert_eq!(tree.place_ghost(3..8), Err(GhostError::PartialOverlap));
}

#[test]
fn revive_at_a_caret_ends_a_nested_ghost_range() {
    let mut tree = Tree::default();
    tree.place(node(1, 0..8, "aa bb cc", "dd")).unwrap();
    tree.place_ghost(3..5).unwrap();

    assert_eq!(tree.revive(&(4..4)), 1);
    assert_eq!(tree.ghost_ranges(), []);
}

#[test]
fn shift_moves_a_slot_after_an_insert_before_it() {
    let mut tree = Tree::default();
    tree.place(node(1, 3..5, "bb", "ee")).unwrap();
    tree.shift(&(0..0), 2, true, None);

    assert_eq!(ranges(&tree), [5..7]);
}

#[test]
fn shift_ends_the_slots_and_ghost_ranges_a_delete_covers() {
    let mut tree = Tree::default();
    tree.place(node(1, 3..5, "bb", "ee")).unwrap();
    tree.place_ghost(6..7).unwrap();
    tree.shift(&(2..8), 0, true, None);

    assert_eq!(tree, Tree::default());
}

#[test]
fn a_writer_edit_inside_an_active_ai_variant_makes_it_mixed() {
    let mut slot = node(1, 3..5, "bb", "ee");
    slot.active = 1;
    let mut tree = Tree::default();
    tree.place(slot).unwrap();
    tree.shift(&(4..4), 1, true, None);

    let shown = &tree.visible_slots()[0];
    assert_eq!(shown.range, 3..6);
    assert_eq!(shown.variants[1].provenance(), Provenance::Mixed);
}

#[test]
fn dissolve_puts_the_nested_slots_and_ghost_ranges_in_place_of_the_slot() {
    let mut tree = Tree::default();
    tree.place(node(1, 0..8, "aa bb cc", "dd")).unwrap();
    tree.place(node(2, 3..5, "bb", "ee")).unwrap();
    tree.place_ghost(6..8).unwrap();
    tree.dissolve(SlotId(1));

    assert_eq!(ranges(&tree), [3..5]);
    assert_eq!(tree.ghosts(), [6..8]);
}

#[test]
fn sync_sets_each_active_variant_to_its_range_of_the_text() {
    let mut tree = Tree::default();
    tree.place(node(1, 3..5, "bb", "ee")).unwrap();
    tree.sync("aa XY cc");

    assert_eq!(tree.visible_slots()[0].variants[0].text(), "XY");
}

#[test]
fn max_id_counts_slots_nested_in_an_inactive_variant() {
    let mut tree = Tree::default();
    tree.place(node(1, 0..8, "aa bb cc", "dd")).unwrap();
    tree.place(node(7, 3..5, "bb", "ee")).unwrap();
    tree.find_mut(SlotId(1)).unwrap().0.active = 1;

    assert_eq!(ranges(&tree), [0..8]);
    assert_eq!(tree.max_id(), 7);
}

#[test]
fn is_free_when_no_slot_overlaps_the_range() {
    let mut tree = Tree::default();
    tree.add(node(1, 3..5, "bb", "ee"));

    assert!(tree.is_free(&(5..8)));
    assert!(!tree.is_free(&(4..6)));
}
