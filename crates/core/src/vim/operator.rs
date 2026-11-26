//! Operators of Vim mode: what `d`, `c`, `y`, `gh`, `gH` and `gs` do with the chars or
//! lines of their span.

use std::ops::Range;

use super::motion::{first_non_blank, line_end, normal_offset};
use super::{Step, Vim, VimCommand, VimMode};
use crate::caret::line_start;
use crate::{Caret, CaretEdit, EditKind};

/// What an operator does with its span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Operator {
    Delete,
    Change,
    Yank,
    Ghost,
    Revive,
    Stash,
}

impl Operator {
    pub(super) fn of(key: char) -> Option<Self> {
        match key {
            'd' => Some(Self::Delete),
            'c' => Some(Self::Change),
            'y' => Some(Self::Yank),
            _ => None,
        }
    }

    /// The operator of `g` and `key`.
    pub(super) fn after_g(key: char) -> Option<Self> {
        match key {
            'h' => Some(Self::Ghost),
            'H' => Some(Self::Revive),
            's' => Some(Self::Stash),
            _ => None,
        }
    }

    /// The key that, typed again, makes the operator act on whole lines: `dd`, `ghgh`.
    pub(super) const fn key(self) -> char {
        match self {
            Self::Delete => 'd',
            Self::Change => 'c',
            Self::Yank => 'y',
            Self::Ghost => 'h',
            Self::Revive => 'H',
            Self::Stash => 's',
        }
    }

    pub(super) const fn after_g_only(self) -> bool {
        matches!(self, Self::Ghost | Self::Revive | Self::Stash)
    }
}

/// The text an operator acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Span {
    /// The chars of a byte range.
    Chars(Range<usize>),
    /// Whole lines: from the line that holds the first offset to the line that holds the
    /// second, in either order.
    Lines(usize, usize),
}

impl Vim {
    /// Runs `operator` over `span` of `text`. The edit is one Undo step; a change
    /// goes on in Insert mode in the same Undo step.
    pub(super) fn operate(&mut self, operator: Operator, span: Span, text: &str) -> Step {
        match span {
            Span::Chars(range) => self.operate_chars(operator, range, text),
            Span::Lines(one, other) => {
                self.operate_lines(operator, one.min(other), one.max(other), text)
            }
        }
    }

    fn operate_chars(&mut self, operator: Operator, range: Range<usize>, text: &str) -> Step {
        let Some(removed) = text.get(range.clone()).map(str::to_owned) else {
            return Step::Done(VimCommand::Nothing);
        };
        match operator {
            Operator::Delete
            | Operator::Yank
            | Operator::Ghost
            | Operator::Revive
            | Operator::Stash
                if removed.is_empty() =>
            {
                Step::Done(VimCommand::Nothing)
            }
            Operator::Ghost => Step::Change(VimCommand::Ghost(range)),
            Operator::Revive => Step::Change(VimCommand::Revive(range)),
            Operator::Stash => Step::Change(VimCommand::Stash(range)),
            Operator::Delete => {
                let mut edit = CaretEdit::new(range.clone(), "", EditKind::Other);
                edit.caret = Caret::at(normal_offset(&replaced(text, &range, ""), range.start));
                Step::Change(VimCommand::Cut { edit, removed })
            }
            Operator::Change => {
                self.mode = VimMode::Insert;
                let edit = CaretEdit::new(range, "", EditKind::Insert);
                Step::Change(VimCommand::Cut { edit, removed })
            }
            Operator::Yank => Step::Done(VimCommand::Yank {
                text: removed,
                caret: Caret::at(range.start),
            }),
        }
    }

    /// `operator` over the lines from the one with `first` to the one with `last`. The
    /// register gets the lines with a newline after each.
    fn operate_lines(&mut self, operator: Operator, first: usize, last: usize, text: &str) -> Step {
        let start = line_start(text, first);
        let end = line_end(text, last);
        let removed = format!("{}\n", text.get(start..end).unwrap_or_default());
        match operator {
            Operator::Delete => {
                let range = with_newline(text, start..end);
                let after = replaced(text, &range, "");
                let line = line_start(&after, range.start.min(after.len()));
                let mut edit = CaretEdit::new(range, "", EditKind::Other);
                edit.caret = Caret::at(first_non_blank(&after, line));
                Step::Change(VimCommand::Cut { edit, removed })
            }
            Operator::Change => {
                self.mode = VimMode::Insert;
                let edit = CaretEdit::new(start..end, "", EditKind::Insert);
                Step::Change(VimCommand::Cut { edit, removed })
            }
            Operator::Yank => Step::Done(VimCommand::Yank {
                text: removed,
                caret: Caret::at(normal_offset(text, first)),
            }),
            Operator::Ghost => Step::Change(VimCommand::Ghost(start..end)),
            Operator::Revive => Step::Change(VimCommand::Revive(start..end)),
            Operator::Stash => Step::Change(VimCommand::Stash(with_newline(text, start..end))),
        }
    }
}

/// The lines in `lines` with the newline after them; the last line takes the newline before it.
fn with_newline(text: &str, lines: Range<usize>) -> Range<usize> {
    if lines.end < text.len() {
        return lines.start..lines.end + 1;
    }
    lines.start.saturating_sub(1)..lines.end
}

/// `text` with `range` replaced by `new`.
pub(super) fn replaced(text: &str, range: &Range<usize>, new: &str) -> String {
    let before = text.get(..range.start).unwrap_or_default();
    let after = text.get(range.end..).unwrap_or_default();
    format!("{before}{new}{after}")
}
