//! The Document as the writer edits it: the selection and the undo steps the writer sees.

use std::ops::Range;

use crate::{Document, EditHistory, EditKind, SlotId};

/// A Document with its selection and its editor undo steps. Each change made through it is
/// one undo step, and the selection follows the change.
#[derive(Debug, Default)]
pub struct EditSession {
    document: Document,
    selection: Range<usize>,
    reversed: bool,
    history: EditHistory,
    /// The undo step the last save wrote (see [`EditHistory::top`]).
    saved: Option<u64>,
}

/// What an undo step did to the Slots, for the Effects the app plays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotChanges {
    /// Slots whose deleted Variant came back, in undo order.
    pub restored: Vec<SlotId>,
    /// The Slot that shows a different Active variant (a cycle, a delete, an undo).
    pub switched: Option<SlotId>,
}

/// What shows of a Slot: its Active variant (index and text) and its Variant count.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ShownVariant {
    slot: SlotId,
    active: usize,
    count: usize,
    text: String,
}

impl ShownVariant {
    fn of(document: &Document) -> Vec<Self> {
        document
            .slots()
            .iter()
            .map(|slot| Self {
                slot: slot.id,
                active: slot.active,
                count: slot.variants.len(),
                text: slot
                    .variants
                    .get(slot.active)
                    .map_or_default(|variant| variant.text().to_owned()),
            })
            .collect()
    }

    /// A different Variant is active than in `old`. Typing in the Active variant changes
    /// only its text; an added Variant changes only the count. Neither is a new Active variant.
    fn replaces(&self, old: &Self) -> bool {
        let moved = self.active != old.active || self.count != old.count;
        moved && self.text != old.text
    }

    /// The first Slot of `document` whose Active variant replaces the one in `before`.
    fn switched(before: &[Self], document: &Document) -> Option<SlotId> {
        Self::of(document)
            .into_iter()
            .find(|shown| {
                before
                    .iter()
                    .any(|old| old.slot == shown.slot && shown.replaces(old))
            })
            .map(|shown| shown.slot)
    }
}

impl EditSession {
    /// A session over `document` with the cursor at the start and no undo steps.
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self {
            document,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Byte range of the selection; empty at the cursor.
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// The Document has changes since the last save or open.
    #[must_use]
    pub fn is_unsaved(&self) -> bool {
        self.history.top() != self.saved
    }

    /// A copy of the Document to save, and the undo step it is at, for
    /// [`Self::mark_saved`]. The next edit starts a new undo step, so it makes the Document
    /// unsaved again.
    pub fn snapshot(&mut self) -> (Document, Option<u64>) {
        self.history.close();
        (self.document.clone(), self.history.top())
    }

    /// Marks undo step `at` (from [`Self::snapshot`]) as the saved one.
    pub fn mark_saved(&mut self, at: Option<u64>) {
        self.saved = at;
    }

    /// Sets the selection; `reversed` puts the cursor at its start. The next edit starts a
    /// new undo step.
    pub fn select(&mut self, selection: Range<usize>, reversed: bool) {
        self.selection = selection;
        self.reversed = reversed;
        self.history.close();
    }

    /// Sets the selection like [`Self::select`], but the undo step stays open: an input
    /// method moves the selection inside the text it is composing.
    pub fn select_in_edit(&mut self, selection: Range<usize>, reversed: bool) {
        self.selection = selection;
        self.reversed = reversed;
    }

    /// The cursor end of the selection.
    #[must_use]
    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    /// The cursor is at the start of the selection.
    #[must_use]
    pub fn is_reversed(&self) -> bool {
        self.reversed
    }

    /// Ends the current undo step: the next edit starts a new one.
    pub fn close_undo_step(&mut self) {
        self.history.close();
    }

    /// Replaces `range` of the text with `text`; the cursor goes after it. Edits of the
    /// same `kind` in a row at the cursor join one undo step. `false` when nothing changed.
    pub fn edit(&mut self, range: Range<usize>, text: &str, kind: EditKind) -> bool {
        let depth = self.document.undo_depth();
        if self.document.replace(range.clone(), text).is_err()
            || self.document.undo_depth() == depth
        {
            return false;
        }
        let before = self.selection.clone();
        let cursor = range.start + text.len();
        self.selection = cursor..cursor;
        self.reversed = false;
        self.history.record(kind, before, self.selection.clone());
        true
    }

    /// Replaces `range` of the Overflow text with `text`. Overflow edits of the same `kind`
    /// in a row join one undo step; the selection stays. `false` when nothing changed.
    pub fn edit_overflow(&mut self, range: Range<usize>, text: &str, kind: EditKind) -> bool {
        let depth = self.document.undo_depth();
        if self.document.edit_overflow(range, text).is_err() || self.document.undo_depth() == depth
        {
            return false;
        }
        self.history
            .record(kind, self.selection.clone(), self.selection.clone());
        true
    }

    /// Runs `change` on the Document as one undo step, however many core steps it makes,
    /// and returns its value with the Slot that shows a new Active variant. The selection
    /// follows the text change. A change that makes no core step makes no undo step. The
    /// steps made before an error still form one undo step.
    ///
    /// # Errors
    /// The error of `change`.
    pub fn change<T, E>(
        &mut self,
        change: impl FnOnce(&mut Document) -> Result<T, E>,
    ) -> Result<(T, Option<SlotId>), E> {
        let selection = self.selection.clone();
        self.change_then_select(change, |old, new| {
            follow_change(selection.start, old, new.text())
                ..follow_change(selection.end, old, new.text())
        })
    }

    /// Like [`Self::change`], but the selection goes to `select(old text, new Document)`.
    ///
    /// # Errors
    /// The error of `change`.
    pub fn change_then_select<T, E>(
        &mut self,
        change: impl FnOnce(&mut Document) -> Result<T, E>,
        select: impl FnOnce(&str, &Document) -> Range<usize>,
    ) -> Result<(T, Option<SlotId>), E> {
        let old_text = self.document.text().to_owned();
        let shown = ShownVariant::of(&self.document);
        let depth = self.document.undo_depth();
        let done = change(&mut self.document);
        let steps = self.document.undo_depth().saturating_sub(depth);
        if steps > 0 {
            let before = self.selection.clone();
            self.selection = select(&old_text, &self.document);
            self.reversed = false;
            self.history
                .record_steps(steps, before, self.selection.clone());
        }
        let done = done?;
        Ok((done, ShownVariant::switched(&shown, &self.document)))
    }

    /// Reverts the last undo step; the selection goes back to where it was before it.
    /// `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Option<SlotChanges> {
        let group = self.history.undo()?;
        let shown = ShownVariant::of(&self.document);
        let mut restored = Vec::new();
        for _ in 0..group.steps {
            restored.extend(self.document.undo_restores_variant());
            self.document.undo();
        }
        self.selection = group.before;
        self.reversed = false;
        Some(SlotChanges {
            restored,
            switched: ShownVariant::switched(&shown, &self.document),
        })
    }

    /// Puts back the last undone undo step; the selection goes to where it was after it.
    /// `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Option<SlotChanges> {
        let group = self.history.redo()?;
        let shown = ShownVariant::of(&self.document);
        for _ in 0..group.steps {
            self.document.redo();
        }
        self.selection = group.after;
        self.reversed = false;
        Some(SlotChanges {
            restored: Vec::new(),
            switched: ShownVariant::switched(&shown, &self.document),
        })
    }
}

/// Where `offset` in `old` lands in `new`, when one span of `old` changed into `new`.
/// An offset inside the changed span goes to the end of the new span. Offsets are bytes;
/// an offset past `old` gives an offset past `new`.
#[must_use]
pub fn follow_change(offset: usize, old: &str, new: &str) -> usize {
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((index, _), _)| index);
    let suffix = old
        .get(prefix..)
        .unwrap_or_default()
        .chars()
        .rev()
        .zip(new.get(prefix..).unwrap_or_default().chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    if offset <= prefix {
        return offset;
    }
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    if offset >= old_end {
        return offset - old_end + new_end;
    }
    new_end
}

#[cfg(test)]
mod tests {
    use super::follow_change;

    #[test]
    fn an_offset_after_a_changed_span_moves_by_its_change_in_length() {
        let old = "It is an eraser here.";
        let new = "It is a thumbtack here.";
        assert_eq!(follow_change(3, old, new), 3);
        assert_eq!(follow_change(21, old, new), 23);
        assert_eq!(follow_change(10, old, new), 17);
    }

    #[test]
    fn a_shared_suffix_does_not_reach_into_the_shared_prefix() {
        assert_eq!(follow_change(1, "aa", "a"), 1);
        assert_eq!(follow_change(2, "aa", "a"), 1);
    }
}
