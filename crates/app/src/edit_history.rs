use std::ops::Range;

/// What an edit was, for grouping into undo steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditKind {
    Typing,
    Deleting,
    OverflowTyping,
    OverflowDeleting,
    Other,
}

/// One undo step of the editor: `steps` core undo steps, and the selection around them.
#[derive(Debug, Clone)]
pub(crate) struct EditGroup {
    id: u64,
    kind: EditKind,
    pub(crate) steps: usize,
    pub(crate) before: Range<usize>,
    pub(crate) after: Range<usize>,
}

/// Groups core edits into editor undo steps. Core undo is per edit; consecutive typing
/// (or deleting) at the cursor is one step for the writer.
#[derive(Debug, Default)]
pub(crate) struct EditHistory {
    undo: Vec<EditGroup>,
    redo: Vec<EditGroup>,
    open: bool,
    next_id: u64,
}

impl EditHistory {
    pub(crate) fn record(&mut self, kind: EditKind, before: Range<usize>, after: Range<usize>) {
        self.redo.clear();
        let open = self.open;
        let joins = |group: &&mut EditGroup| {
            open && kind != EditKind::Other && group.kind == kind && group.after == before
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
            steps: 1,
            before,
            after,
        });
        self.open = true;
    }

    /// The id of the last undo step; `None` when there is none. Equal ids: the same
    /// Document state, so a save can mark it.
    pub(crate) fn top(&self) -> Option<u64> {
        self.undo.last().map(|group| group.id)
    }

    /// Ends the current group: the next edit starts a new undo step.
    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn undo(&mut self) -> Option<EditGroup> {
        self.open = false;
        let group = self.undo.pop()?;
        self.redo.push(group.clone());
        Some(group)
    }

    pub(crate) fn redo(&mut self) -> Option<EditGroup> {
        self.open = false;
        let group = self.redo.pop()?;
        self.undo.push(group.clone());
        Some(group)
    }
}
