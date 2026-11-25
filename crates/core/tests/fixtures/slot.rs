use std::ops::Range;

use hamish_core::{Document, Level, Provenance, SlotId};

/// Creates a Slot over `range` with one Writer Variant; panics if the Document refuses it.
#[expect(clippy::unwrap_used, reason = "a refused fixture Slot fails the test")]
pub fn writer_slot(
    document: &mut Document,
    range: Range<usize>,
    level: Level,
    variant: &str,
) -> SlotId {
    document
        .create_slot(range, level, variant, Provenance::Writer)
        .unwrap()
}
