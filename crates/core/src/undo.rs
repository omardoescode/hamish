//! Core undo steps of the Document: each change of text, Tree, Overflow, and Lab marks,
//! and the stacks that undo and redo them.

use std::ops::Range;

use crate::tree::Tree;
use crate::{LabMark, Overflow, SlotId};

/// The Tree, Overflow, and Lab marks of the Document at one time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Snapshot {
    pub(crate) tree: Tree,
    pub(crate) overflow: Overflow,
    pub(crate) lab_marks: Vec<LabMark>,
}

/// One undoable change: `removed` at `at` was swapped for `inserted`, and the rest of the
/// Document went from `before` to `after`.
#[derive(Debug, Clone)]
pub(crate) struct Step {
    pub(crate) at: usize,
    pub(crate) removed: String,
    pub(crate) inserted: String,
    pub(crate) before: Snapshot,
    pub(crate) after: Snapshot,
    /// The step deleted a Variant of this Slot (its undo plays a burst).
    pub(crate) variant_deleted: Option<SlotId>,
}

impl Step {
    /// The byte range the inserted text takes.
    pub(crate) fn inserted_range(&self) -> Range<usize> {
        self.at..self.at + self.inserted.len()
    }

    /// The step that reverts this one.
    pub(crate) fn inverse(self) -> Self {
        Self {
            at: self.at,
            removed: self.inserted,
            inserted: self.removed,
            before: self.after,
            after: self.before,
            variant_deleted: self.variant_deleted,
        }
    }
}

/// The undo and redo stacks of core undo steps.
#[derive(Debug, Clone, Default)]
pub(crate) struct UndoSteps {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl UndoSteps {
    /// Adds a new step to undo. The redo stack clears.
    pub(crate) fn push(&mut self, step: Step) {
        self.undo.push(step);
        self.redo.clear();
    }

    /// How many steps there are to undo.
    pub(crate) fn depth(&self) -> usize {
        self.undo.len()
    }

    /// Marks the undo step at `depth` as a Variant delete of Slot `id`. No step there: no change.
    pub(crate) fn mark_variant_deleted(&mut self, depth: usize, id: SlotId) {
        if let Some(step) = self.undo.get_mut(depth) {
            step.variant_deleted = Some(id);
        }
    }

    /// The Slot of the Variant delete that the next undo reverts.
    pub(crate) fn undo_restores_variant(&self) -> Option<SlotId> {
        self.undo.last()?.variant_deleted
    }

    /// The Slot of the Variant delete that the next redo applies.
    pub(crate) fn redo_deletes_variant(&self) -> Option<SlotId> {
        self.redo.last()?.variant_deleted
    }

    /// Takes the last step to undo. `None` when there is none.
    pub(crate) fn take_undo(&mut self) -> Option<Step> {
        self.undo.pop()
    }

    /// Takes the last step to redo. `None` when there is none.
    pub(crate) fn take_redo(&mut self) -> Option<Step> {
        self.redo.pop()
    }

    /// Puts the step that reverts an undo on the redo stack.
    pub(crate) fn undone(&mut self, step: Step) {
        self.redo.push(step);
    }

    /// Puts the step that reverts a redo on the undo stack. The redo stack stays.
    pub(crate) fn redone(&mut self, step: Step) {
        self.undo.push(step);
    }
}
