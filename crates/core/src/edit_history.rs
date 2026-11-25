//! Groups core edits into the undo steps the writer sees: consecutive typing or deleting at
//! the caret is one step.

use std::ops::Range;

/// What an edit was, for grouping into undo steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Deleting,
    /// An edit that joins no other.
    Other,
}

/// The text an edit changed. Edits in different texts never join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditedText {
    Document,
    Overflow,
}

/// One undo step of the editor: `steps` core undo steps, and the selection around them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditGroup {
    id: u64,
    kind: EditKind,
    text: EditedText,
    /// How many core undo steps the group holds.
    pub steps: usize,
    /// Byte range of the selection before the first edit.
    pub before: Range<usize>,
    /// Byte range of the selection after the last edit.
    pub after: Range<usize>,
}

/// The undo and redo stacks of editor undo steps. Core undo is per edit; this joins edits.
#[derive(Debug, Default)]
pub struct EditHistory {
    undo: Vec<EditGroup>,
    redo: Vec<EditGroup>,
    open: bool,
    next_id: u64,
}

impl EditHistory {
    /// Records one core undo step of `kind` in `text`. It joins the last group when that
    /// group is open, has the same kind (not [`EditKind::Other`]) in the same text, and ends
    /// where this edit starts. The redo stack clears.
    pub fn record(
        &mut self,
        kind: EditKind,
        text: EditedText,
        before: Range<usize>,
        after: Range<usize>,
    ) {
        self.redo.clear();
        let open = self.open;
        let joins = |group: &&mut EditGroup| {
            open && kind != EditKind::Other
                && group.kind == kind
                && group.text == text
                && group.after == before
        };
        if let Some(group) = self.undo.last_mut().filter(joins) {
            group.steps += 1;
            group.after = after;
            return;
        }
        self.next_id += 1;
        self.undo.push(EditGroup {
            id: self.next_id,
            kind,
            text,
            steps: 1,
            before,
            after,
        });
        self.open = true;
    }

    /// Records `steps` core undo steps as one editor undo step that joins no other.
    pub fn record_steps(&mut self, steps: usize, before: Range<usize>, after: Range<usize>) {
        self.close();
        self.record(EditKind::Other, EditedText::Document, before, after);
        if let Some(group) = self.undo.last_mut() {
            group.steps = steps;
        }
        self.close();
    }

    /// The id of the last undo step; `None` when there is none. Equal ids mean the same
    /// Document state, so a save can mark it.
    #[must_use]
    pub fn top(&self) -> Option<u64> {
        self.undo.last().map(|group| group.id)
    }

    /// Ends the current group: the next edit starts a new undo step.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Moves the last undo step to the redo stack and returns it. The caller undoes its
    /// `steps` core steps. `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Option<EditGroup> {
        self.open = false;
        let group = self.undo.pop()?;
        self.redo.push(group.clone());
        Some(group)
    }

    /// Moves the last redo step back to the undo stack and returns it. The caller redoes its
    /// `steps` core steps. `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Option<EditGroup> {
        self.open = false;
        let group = self.redo.pop()?;
        self.undo.push(group.clone());
        Some(group)
    }
}
