//! The Document as the writer edits it: the selection and the undo steps the writer sees.

use std::collections::HashMap;
use std::ops::Range;

use crate::{Caret, CaretEdit, Document, EditHistory, EditedText, SlotId, word_at};

/// A Document with its selection and its editor undo steps. Each change made through it is
/// one undo step, and the selection follows the change.
#[derive(Debug, Default)]
pub struct EditSession {
    document: Document,
    caret: Caret,
    history: EditHistory,
    /// The undo step the last save wrote (see [`EditHistory::top`]).
    saved: Option<u64>,
}

/// What an undo step did to the Slots, for the Effects the app plays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotChanges {
    /// Slots whose deleted Variant came back, in undo order.
    pub restored: Vec<SlotId>,
    /// Spans whose Active variant a redo deleted, as they were before the redo, in redo
    /// order.
    pub deleted: Vec<Range<usize>>,
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
        let before: HashMap<SlotId, &Self> = before.iter().map(|old| (old.slot, old)).collect();
        Self::of(document)
            .into_iter()
            .find(|shown| {
                before
                    .get(&shown.slot)
                    .is_some_and(|old| shown.replaces(old))
            })
            .map(|shown| shown.slot)
    }
}

impl EditSession {
    /// A session over `document` with the caret at the start and no undo steps.
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

    #[must_use]
    pub fn caret(&self) -> &Caret {
        &self.caret
    }

    /// Byte range of the selection; empty at the caret.
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.caret.selection()
    }

    /// What the menu actions act on: the selection, else the word at the caret, else the
    /// caret.
    #[must_use]
    pub fn target(&self) -> Range<usize> {
        let selection = self.selection();
        if !selection.is_empty() {
            return selection;
        }
        let caret = self.caret.offset();
        word_at(self.document.text(), caret).unwrap_or(caret..caret)
    }

    /// Counter text: the Document, or the selection with "selected".
    #[must_use]
    pub fn counter_label(&self) -> String {
        let selection = self.selection();
        if selection.is_empty() {
            return self.document.counter().to_string();
        }
        match self.document.selection_counter(selection) {
            Ok(counter) => format!("{counter} selected"),
            Err(_) => self.document.counter().to_string(),
        }
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
        (self.document.without_history(), self.history.top())
    }

    /// Marks undo step `at` (from [`Self::snapshot`]) as the saved one.
    pub fn mark_saved(&mut self, at: Option<u64>) {
        self.saved = at;
    }

    /// Puts the caret in the Document text. The next edit starts a new undo step.
    pub fn select(&mut self, caret: Caret) {
        self.caret = caret;
        self.history.close();
    }

    /// Ends the composed text of an input method; the undo step stays open.
    pub fn unmark(&mut self) {
        self.caret = self.caret.unmarked();
    }

    /// Ends the current undo step: the next edit starts a new one.
    pub fn close_undo_step(&mut self) {
        self.history.close();
    }

    /// Makes `edit` in the Document text; its caret shows after it. Edits of the same kind
    /// in a row at the caret join one undo step. `false` when nothing changed.
    pub fn edit(&mut self, edit: &CaretEdit) -> bool {
        let depth = self.document.undo_depth();
        if self
            .document
            .replace(edit.range.clone(), &edit.text)
            .is_err()
            || self.document.undo_depth() == depth
        {
            return false;
        }
        let before = self.caret.selection();
        self.caret = edit.caret.clone();
        self.history.record(
            edit.kind,
            EditedText::Document,
            before,
            self.caret.selection(),
        );
        true
    }

    /// Makes `edit` in the Overflow text; the Document caret stays. Overflow edits of the
    /// same kind in a row join one undo step, apart from Document edits. `false` when
    /// nothing changed.
    pub fn edit_overflow(&mut self, edit: &CaretEdit) -> bool {
        let depth = self.document.undo_depth();
        if self
            .document
            .edit_overflow(edit.range.clone(), &edit.text)
            .is_err()
            || self.document.undo_depth() == depth
        {
            return false;
        }
        let selection = self.caret.selection();
        self.history.record(
            edit.kind,
            EditedText::Overflow,
            selection.clone(),
            selection,
        );
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
        let caret = self.caret.clone();
        self.change_then_select(change, |old, new| {
            caret.followed(old, new.text()).selection()
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
            let before = self.caret.selection();
            self.caret = Caret::over(select(&old_text, &self.document));
            self.history
                .record_steps(steps, before, self.caret.selection());
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
        self.caret = Caret::over(group.before.clone());
        Some(SlotChanges {
            restored,
            deleted: Vec::new(),
            switched: ShownVariant::switched(&shown, &self.document),
        })
    }

    /// Puts back the last undone undo step; the selection goes to where it was after it.
    /// `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Option<SlotChanges> {
        let group = self.history.redo()?;
        let shown = ShownVariant::of(&self.document);
        let mut deleted = Vec::new();
        for _ in 0..group.steps {
            let span = self
                .document
                .redo_deletes_variant()
                .and_then(|id| self.document.slot(id))
                .map(|slot| slot.range);
            deleted.extend(span);
            self.document.redo();
        }
        self.caret = Caret::over(group.after.clone());
        Some(SlotChanges {
            restored: Vec::new(),
            deleted,
            switched: ShownVariant::switched(&shown, &self.document),
        })
    }
}
