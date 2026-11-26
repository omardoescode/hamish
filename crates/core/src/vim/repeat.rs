//! Dot repeat of Vim mode: the last change, and how `.` runs it again from the caret.

use unicode_segmentation::UnicodeSegmentation;

use super::motion::{at_column, column_of, lines_down, right};
use super::operator::replaced;
use super::{Step, Vim, VimCommand, VimKey, VimMode, VimText, count_digit, step_back};
use crate::{Caret, CaretEdit, EditKind};

/// A change that `.` repeats: its count, its keys after the count, the text typed
/// in the Insert mode it started, and the size of its Visual selection.
#[derive(Debug, Clone, Default)]
pub(super) struct LastChange {
    count: Option<usize>,
    keys: Vec<VimKey>,
    pub(super) inserted: String,
    /// `None` for a change made in Normal mode.
    pub(super) visual: Option<VisualSize>,
}

/// How much text a Visual mode change acted on, so that `.` acts on as much from the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VisualSize {
    /// Visual mode in one line: the head is this many graphemes after the anchor.
    Chars(usize),
    /// Visual mode over more lines: the head is `lines` lines down, at `column`.
    Lines { lines: usize, column: usize },
    /// Visual line mode: the head is this many lines down.
    WholeLines(usize),
}

impl VisualSize {
    /// The size of the selection from `anchor` to `head` in `mode`.
    pub(super) fn of(mode: VimMode, text: &str, anchor: usize, head: usize) -> Self {
        let (low, high) = (anchor.min(head), anchor.max(head));
        let between = text.get(low..high).unwrap_or_default();
        let lines = between.matches('\n').count();
        if mode == VimMode::VisualLine {
            return Self::WholeLines(lines);
        }
        if lines == 0 {
            return Self::Chars(between.graphemes(true).count());
        }
        Self::Lines {
            lines,
            column: column_of(text, high),
        }
    }

    /// The mode and the head char of a selection of this size from `anchor`.
    fn head(self, text: &str, anchor: usize) -> (VimMode, usize) {
        match self {
            Self::Chars(after) => (VimMode::Visual, right(text, anchor, after)),
            Self::Lines { lines, column } => (
                VimMode::Visual,
                at_column(text, lines_down(text, anchor, lines), column),
            ),
            Self::WholeLines(lines) => (VimMode::VisualLine, lines_down(text, anchor, lines)),
        }
    }
}

impl Vim {
    pub(super) fn record(&mut self, typed: Vec<VimKey>, command: &VimCommand) {
        let mut count: Option<usize> = None;
        let mut keys = typed.into_iter().peekable();
        while let Some(digit) = keys
            .peek()
            .and_then(|key| count_digit(*key, count.is_some()))
        {
            count = Some(count.unwrap_or(0).saturating_mul(10).saturating_add(digit));
            keys.next();
        }
        self.last = Some(LastChange {
            count,
            keys: keys.collect(),
            ..LastChange::default()
        });
        self.insert_start = match command {
            _ if self.mode != VimMode::Insert => None,
            VimCommand::Edit(edit) | VimCommand::Cut { edit, .. } => Some(edit.caret.offset()),
            VimCommand::Select(caret) => Some(caret.offset()),
            VimCommand::PassOn
            | VimCommand::Nothing
            | VimCommand::Yank { .. }
            | VimCommand::Put(_)
            | VimCommand::Undo(_)
            | VimCommand::Redo(_)
            | VimCommand::Ghost(_)
            | VimCommand::Revive(_)
            | VimCommand::Stash(_)
            | VimCommand::CycleSlot { .. }
            | VimCommand::Run(_) => None,
        };
    }

    /// `.`: runs the keys of the last change again, with `count` in place of its count,
    /// and puts in the text typed after it as one edit. A Visual mode change acts on a
    /// selection of the same size from `offset`, with no count.
    pub(super) fn repeat_change(
        &mut self,
        count: Option<usize>,
        text: &VimText<'_>,
        offset: usize,
    ) -> Step {
        let Some(last) = self.last.clone() else {
            return Step::Done(VimCommand::Nothing);
        };
        self.typed.clear();
        self.replaying = true;
        let command = match last.visual {
            Some(size) => self.replay_visual(size, &last.keys, text, offset),
            None => self.replay_normal(count.or(last.count), &last.keys, text, offset),
        };
        self.replaying = false;
        if self.mode != VimMode::Insert {
            return Step::Done(command);
        }
        self.mode = VimMode::Normal;
        Step::Done(with_inserted(command, &last.inserted, text.text))
    }

    fn replay_normal(
        &mut self,
        count: Option<usize>,
        keys: &[VimKey],
        text: &VimText<'_>,
        offset: usize,
    ) -> VimCommand {
        let digits = count.map_or_else(String::new, |count| count.to_string());
        let mut command = VimCommand::Nothing;
        for key in digits.chars().map(VimKey::Char).chain(keys.iter().copied()) {
            command = self.normal_key(key, text, offset);
        }
        command
    }

    /// Runs `keys` in Visual mode over a selection of `size` from `offset`. The selection
    /// of `gv` stays.
    fn replay_visual(
        &mut self,
        size: VisualSize,
        keys: &[VimKey],
        text: &VimText<'_>,
        offset: usize,
    ) -> VimCommand {
        let last_visual = self.last_visual;
        let (mode, head) = size.head(text.text, offset);
        self.mode = mode;
        self.anchor = offset;
        self.head = head;
        let mut command = VimCommand::Nothing;
        for &key in keys {
            command = self.visual_key(key, text, self.head);
        }
        self.last_visual = last_visual;
        command
    }
}

/// The edit of a change that started Insert mode, with `inserted` typed at its caret, back
/// in Normal mode.
fn with_inserted(command: VimCommand, inserted: &str, text: &str) -> VimCommand {
    let (edit, removed) = match command {
        VimCommand::Edit(edit) => (edit, None),
        VimCommand::Cut { edit, removed } => (edit, Some(removed)),
        VimCommand::Select(caret) => {
            let at = caret.offset();
            (CaretEdit::new(at..at, "", EditKind::Other), None)
        }
        other @ (VimCommand::PassOn
        | VimCommand::Nothing
        | VimCommand::Yank { .. }
        | VimCommand::Put(_)
        | VimCommand::Undo(_)
        | VimCommand::Redo(_)
        | VimCommand::Ghost(_)
        | VimCommand::Revive(_)
        | VimCommand::Stash(_)
        | VimCommand::CycleSlot { .. }
        | VimCommand::Run(_)) => return other,
    };
    let split = edit.caret.offset().saturating_sub(edit.range.start);
    let before = edit.text.get(..split).unwrap_or(&edit.text);
    let after = edit.text.get(split..).unwrap_or_default();
    let new = format!("{before}{inserted}{after}");
    let end = edit.range.start + before.len() + inserted.len();
    let mut done = CaretEdit::new(edit.range.clone(), &new, EditKind::Other);
    done.caret = Caret::at(step_back(&replaced(text, &edit.range, &new), end));
    match removed {
        Some(removed) => VimCommand::Cut {
            edit: done,
            removed,
        },
        None => VimCommand::Edit(done),
    }
}
