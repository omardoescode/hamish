use std::fmt;
use std::ops::Range;

use crate::slot::{self, SlotNode};
use crate::{Counter, Cycle, Level, Provenance, Slot, SlotError, SlotId, Variant};

/// One piece of writing: its text, Slots, and edit history.
#[derive(Debug, Clone, Default)]
pub struct Document {
    text: String,
    slots: Vec<SlotNode>,
    next_slot: SlotId,
    undo_steps: Vec<Step>,
    redo_steps: Vec<Step>,
}

/// How a Slot change reaches the text. Only a writer text edit makes an
/// outer AI Variant Mixed; a Variant switch does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotChange {
    Switch,
    WriterText,
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
    /// The Original stays the Active variant. A range inside a visible Slot makes a Slot
    /// nested in that Slot's Active variant (ADR 0006): it shows only while that Variant does.
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
        self.create_slot_with(range, level, &[text], provenance)
    }

    /// Makes a Slot over `range`: its text becomes the Original, `texts` the next Variants,
    /// as one undoable step. The Original stays the Active variant.
    pub(crate) fn create_slot_with(
        &mut self,
        range: Range<usize>,
        level: Level,
        texts: &[&str],
        provenance: Provenance,
    ) -> Result<SlotId, SlotError> {
        let original = self.text.get(range.clone()).ok_or(RangeError {
            range: range.clone(),
        })?;
        if range.is_empty() {
            return Err(SlotError::EmptyRange);
        }
        let id = self.next_slot;
        let variants = std::iter::once(Variant::new(original, Provenance::Writer))
            .chain(texts.iter().map(|text| Variant::new(text, provenance)))
            .collect();
        let node = SlotNode {
            id,
            level,
            range: range.clone(),
            variants,
            active: 0,
        };
        let mut slots = self.slots.clone();
        slot::place(&mut slots, node)?;
        self.commit(range.start..range.start, "", slots)?;
        self.next_slot = id.next();
        Ok(id)
    }

    /// Adds a Variant at the end of Slot `id`. The Active variant stays.
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] when `id` is not a visible Slot.
    pub fn add_variant(
        &mut self,
        id: SlotId,
        text: &str,
        provenance: Provenance,
    ) -> Result<(), SlotError> {
        self.change_slot(id, SlotChange::Switch, |node| {
            node.push_variant(text, provenance);
            Ok(())
        })
    }

    /// Adds `texts` as Variants at the end of Slot `id`, as one undoable step.
    pub(crate) fn add_variants(
        &mut self,
        id: SlotId,
        texts: &[&str],
        provenance: Provenance,
    ) -> Result<(), SlotError> {
        self.change_slot(id, SlotChange::Switch, |node| {
            for text in texts {
                node.push_variant(text, provenance);
            }
            Ok(())
        })
    }

    /// Makes Variant `index` of Slot `id` the Active variant (FR-15, FR-16).
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] or [`SlotError::NoVariant`] when they do not exist.
    pub fn set_active(&mut self, id: SlotId, index: usize) -> Result<(), SlotError> {
        self.change_slot(id, SlotChange::Switch, |node| node.set_active(index))
    }

    /// Moves the Active variant of Slot `id` one step (FR-14).
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] when `id` is not a visible Slot.
    pub fn cycle_active(&mut self, id: SlotId, cycle: Cycle) -> Result<(), SlotError> {
        let slot = self.slot(id).ok_or(SlotError::NoSlot(id))?;
        let count = slot.variants.len();
        let step = match cycle {
            Cycle::Next => 1,
            Cycle::Previous => count.saturating_sub(1),
        };
        let index = (slot.active + step).checked_rem(count).unwrap_or(0);
        self.set_active(id, index)
    }

    /// Deletes Variant `index` of Slot `id`. When it was active, the Original shows.
    /// A Slot left with fewer than two Variants ends; its active text stays (FR-18).
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] or [`SlotError::NoVariant`] when they do not exist.
    pub fn delete_variant(&mut self, id: SlotId, index: usize) -> Result<(), SlotError> {
        self.change_slot(id, SlotChange::Switch, |node| node.remove_variant(index))
    }

    /// Sets the text of Variant `index` of Slot `id`, as a writer edit:
    /// an AI Variant becomes Mixed. Slots nested in that Variant end.
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] or [`SlotError::NoVariant`] when they do not exist.
    pub fn edit_variant(&mut self, id: SlotId, index: usize, text: &str) -> Result<(), SlotError> {
        self.change_slot(id, SlotChange::WriterText, |node| {
            node.edit_variant(index, text)
        })
    }

    /// Runs `change` on Slot `id`, then shows its Active variant text in its range,
    /// as one undoable step.
    fn change_slot(
        &mut self,
        id: SlotId,
        kind: SlotChange,
        change: impl FnOnce(&mut SlotNode) -> Result<(), SlotError>,
    ) -> Result<(), SlotError> {
        let mut slots = self.slots.clone();
        let (node, base) = slot::find_mut(&mut slots, id, 0).ok_or(SlotError::NoSlot(id))?;
        let old = base + node.range.start..base + node.range.end;
        change(node)?;
        let shown = node.active_text()?.to_owned();
        node.range.end = node.range.start + shown.len();
        let writer_edit =
            kind == SlotChange::WriterText && self.text.get(old.clone()) != Some(shown.as_str());
        let ends = node.variants.len() < 2;
        slot::shift(&mut slots, &old, shown.len(), writer_edit, Some(id));
        if ends {
            slot::dissolve(&mut slots, id);
        }
        self.commit(old, &shown, slots)?;
        Ok(())
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
        let writer_edit = removed != text;
        let mut slots = self.slots.clone();
        slot::shift(&mut slots, &range, text.len(), writer_edit, None);
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
        let mut text = self.text.clone();
        text.replace_range(range.clone(), inserted);
        slot::sync(&mut slots, &text, 0);
        if text == self.text && slots == self.slots {
            return Ok(());
        }
        self.text = text;
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
