use std::fmt;
use std::ops::Range;

use crate::slot::{self, SlotNode};
use crate::{Counter, Level, Provenance, Slot, SlotError, SlotId, Variant};

/// One piece of writing: its text, Slots, and edit history.
#[derive(Debug, Clone, Default)]
pub struct Document {
    text: String,
    slots: Vec<SlotNode>,
    next_slot: SlotId,
    undo_steps: Vec<Step>,
    redo_steps: Vec<Step>,
}

/// One undoable change: `removed` at `at` was swapped for `inserted`,
/// and the Slots went from `slots_before` to `slots_after`.
#[derive(Debug, Clone)]
struct Step {
    at: usize,
    removed: String,
    inserted: String,
    slots_before: Vec<SlotNode>,
    slots_after: Vec<SlotNode>,
}

impl Step {
    fn inverse(self) -> Self {
        Self {
            at: self.at,
            removed: self.inserted,
            inserted: self.removed,
            slots_before: self.slots_after,
            slots_after: self.slots_before,
        }
    }
}

/// A byte range or offset that is out of bounds or not on a char boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeError {
    pub range: Range<usize>,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid text range {}..{}",
            self.range.start, self.range.end
        )
    }
}

impl std::error::Error for RangeError {}

impl Document {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn counter(&self) -> Counter {
        Counter::of(&self.text)
    }

    /// The first line, shown as the title (FR-5). `None` when the first line is blank.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.text
            .lines()
            .next()
            .filter(|line| !line.trim().is_empty())
    }

    /// Counter of the text in byte range `selection`.
    ///
    /// # Errors
    /// [`RangeError`] when `selection` is out of bounds, reversed, or not on char boundaries.
    pub fn selection_counter(&self, selection: Range<usize>) -> Result<Counter, RangeError> {
        self.text
            .get(selection.clone())
            .map(Counter::of)
            .ok_or(RangeError { range: selection })
    }

    /// The visible Slots in document order, each parent before the Slots nested in it.
    #[must_use]
    pub fn slots(&self) -> Vec<Slot<'_>> {
        let mut out = Vec::new();
        slot::collect(&self.slots, 0, &mut out);
        out
    }

    /// The visible Slot `id`. `None` when it ended or sits in an inactive Variant.
    #[must_use]
    pub fn slot(&self, id: SlotId) -> Option<Slot<'_>> {
        self.slots().into_iter().find(|slot| slot.id == id)
    }

    /// Makes a Slot over `range`: its text becomes the Original, `text` the second Variant.
    /// The Original stays the Active variant.
    ///
    /// # Errors
    /// [`SlotError::Range`] for an invalid range, [`SlotError::EmptyRange`] for an empty one,
    /// [`SlotError::PartialOverlap`] when `range` partly overlaps another Slot.
    pub fn create_slot(
        &mut self,
        range: Range<usize>,
        level: Level,
        text: &str,
        provenance: Provenance,
    ) -> Result<SlotId, SlotError> {
        let original = self.text.get(range.clone()).ok_or(RangeError {
            range: range.clone(),
        })?;
        if range.is_empty() {
            return Err(SlotError::EmptyRange);
        }
        let id = self.next_slot;
        let node = SlotNode {
            id,
            level,
            range: range.clone(),
            variants: vec![
                Variant::new(original, Provenance::Writer),
                Variant::new(text, provenance),
            ],
            active: 0,
        };
        let mut slots = self.slots.clone();
        slot::place(&mut slots, node)?;
        self.commit(range.start..range.start, "", slots)?;
        self.next_slot = id.next();
        Ok(id)
    }

    /// Inserts `text` at byte offset `at`.
    ///
    /// # Errors
    /// [`RangeError`] when `at` is out of bounds or not on a char boundary.
    pub fn insert(&mut self, at: usize, text: &str) -> Result<(), RangeError> {
        self.replace(at..at, text)
    }

    /// Removes the text in byte range `range`.
    ///
    /// # Errors
    /// [`RangeError`] when `range` is out of bounds, reversed, or not on char boundaries.
    pub fn delete(&mut self, range: Range<usize>) -> Result<(), RangeError> {
        self.replace(range, "")
    }

    /// Replaces the text in byte range `range` with `text`.
    ///
    /// # Errors
    /// [`RangeError`] when `range` is out of bounds, reversed, or not on char boundaries.
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Result<(), RangeError> {
        let removed = self.text.get(range.clone()).ok_or(RangeError {
            range: range.clone(),
        })?;
        if removed.is_empty() && text.is_empty() {
            return Ok(());
        }
        let slots = self.slots.clone();
        self.commit(range, text, slots)
    }

    /// Replaces `range` with `inserted` and sets the Slots to `slots`, as one undoable step.
    fn commit(
        &mut self,
        range: Range<usize>,
        inserted: &str,
        mut slots: Vec<SlotNode>,
    ) -> Result<(), RangeError> {
        let removed = self
            .text
            .get(range.clone())
            .ok_or(RangeError {
                range: range.clone(),
            })?
            .to_owned();
        self.text.replace_range(range.clone(), inserted);
        slot::sync(&mut slots, &self.text, 0);
        let slots_before = std::mem::replace(&mut self.slots, slots);
        self.undo_steps.push(Step {
            at: range.start,
            removed,
            inserted: inserted.to_owned(),
            slots_before,
            slots_after: self.slots.clone(),
        });
        self.redo_steps.clear();
        Ok(())
    }

    /// Reverts the last step. `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo_steps.pop() else {
            return false;
        };
        let step = self.apply(step);
        self.redo_steps.push(step);
        true
    }

    /// Applies the last undone step again. `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo_steps.pop() else {
            return false;
        };
        let step = self.apply(step);
        self.undo_steps.push(step);
        true
    }

    /// Reverts `step` and returns the step that reverts it back.
    fn apply(&mut self, step: Step) -> Step {
        let end = step.at + step.inserted.len();
        self.text.replace_range(step.at..end, &step.removed);
        self.slots.clone_from(&step.slots_before);
        step.inverse()
    }
}
