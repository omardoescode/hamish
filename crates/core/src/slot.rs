//! Slots and Variants: Level, Provenance, Slot ids, and the Variant changes of one Slot.

use std::cmp::Ordering;
use std::fmt;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::RangeError;
use crate::tree::Tree;

/// How much text a Slot covers. Word also covers short phrases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Word,
    Sentence,
    Paragraph,
}

/// Who wrote a Variant. Mixed: an AI Variant the writer edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Writer,
    Ai,
    Mixed,
}

/// Direction to cycle the Active variant. Both wrap around.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cycle {
    Next,
    Previous,
}

/// What a change of the Active variant did: it showed the Original again, or another
/// Variant. The app picks the cycle sound from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    ToOriginal,
    ToVariant,
}

impl Switch {
    pub(crate) fn to(index: usize) -> Self {
        if index == 0 {
            Self::ToOriginal
        } else {
            Self::ToVariant
        }
    }
}

/// Stable name of a Slot for the life of its Document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotId(pub(crate) u64);

impl SlotId {
    pub(crate) fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for SlotId {
    fn default() -> Self {
        Self(1)
    }
}

/// One candidate text for a Slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub(crate) text: String,
    pub(crate) provenance: Provenance,
    pub(crate) tree: Tree,
}

impl Variant {
    pub(crate) fn new(text: &str, provenance: Provenance) -> Self {
        Self {
            text: text.to_owned(),
            provenance,
            tree: Tree::default(),
        }
    }

    /// A writer edit to the text of an AI Variant makes it Mixed.
    pub(crate) fn mark_edited(&mut self) {
        if self.provenance == Provenance::Ai {
            self.provenance = Provenance::Mixed;
        }
    }

    /// The text of the Variant.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Who wrote the Variant.
    #[must_use]
    pub fn provenance(&self) -> Provenance {
        self.provenance
    }
}

/// A Slot as the Document shows it now. `range` is a byte range of the active text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot<'a> {
    pub id: SlotId,
    pub level: Level,
    pub range: Range<usize>,
    /// The Original first, then the other Variants in the order they were added.
    pub variants: &'a [Variant],
    /// Index of the Active variant in `variants`.
    pub active: usize,
}

/// Why a Slot operation was refused. A refused operation changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotError {
    Range(RangeError),
    EmptyRange,
    /// The range partly overlaps another Slot or a Ghost range (strict nesting, ADR 0006).
    PartialOverlap,
    NoSlot(SlotId),
    NoVariant {
        slot: SlotId,
        index: usize,
    },
}

impl fmt::Display for SlotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range(error) => error.fmt(f),
            Self::EmptyRange => f.write_str("a Slot needs a non-empty range"),
            Self::PartialOverlap => {
                f.write_str("the range partly overlaps another Slot or a Ghost range")
            }
            Self::NoSlot(id) => write!(f, "no Slot {}", id.0),
            Self::NoVariant { slot, index } => write!(f, "Slot {} has no Variant {index}", slot.0),
        }
    }
}

impl std::error::Error for SlotError {}

impl From<RangeError> for SlotError {
    fn from(error: RangeError) -> Self {
        Self::Range(error)
    }
}

/// A Slot in the tree. `range` is relative to the start of the text that holds it:
/// the Document for a top-level Slot, else the Active variant of the parent Slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotNode {
    pub(crate) id: SlotId,
    pub(crate) level: Level,
    pub(crate) range: Range<usize>,
    pub(crate) variants: Vec<Variant>,
    pub(crate) active: usize,
}

impl SlotNode {
    pub(crate) fn active_text(&self) -> Result<&str, SlotError> {
        self.variants
            .get(self.active)
            .map(|variant| variant.text.as_str())
            .ok_or(self.no_variant(self.active))
    }

    pub(crate) fn no_variant(&self, index: usize) -> SlotError {
        SlotError::NoVariant {
            slot: self.id,
            index,
        }
    }

    pub(crate) fn set_active(&mut self, index: usize) -> Result<(), SlotError> {
        if index >= self.variants.len() {
            return Err(self.no_variant(index));
        }
        self.active = index;
        Ok(())
    }

    pub(crate) fn push_variant(&mut self, text: &str, provenance: Provenance) {
        self.variants.push(Variant::new(text, provenance));
    }

    /// Removes Variant `index`. When it was active, the next Variant becomes active;
    /// after the last one, the first.
    pub(crate) fn remove_variant(&mut self, index: usize) -> Result<(), SlotError> {
        if index >= self.variants.len() {
            return Err(self.no_variant(index));
        }
        self.variants.remove(index);
        self.active = match index.cmp(&self.active) {
            Ordering::Less => self.active - 1,
            Ordering::Equal if index < self.variants.len() => index,
            Ordering::Equal => 0,
            Ordering::Greater => self.active,
        };
        Ok(())
    }

    /// A writer edit of Variant `index`. The Slots and Ghost ranges nested in it end.
    pub(crate) fn edit_variant(&mut self, index: usize, text: &str) -> Result<(), SlotError> {
        let error = self.no_variant(index);
        let variant = self.variants.get_mut(index).ok_or(error)?;
        if variant.text == text {
            return Ok(());
        }
        text.clone_into(&mut variant.text);
        variant.tree = Tree::default();
        variant.mark_edited();
        Ok(())
    }

    pub(crate) fn active_mut(&mut self) -> Result<&mut Variant, SlotError> {
        let error = self.no_variant(self.active);
        self.variants.get_mut(self.active).ok_or(error)
    }
}
