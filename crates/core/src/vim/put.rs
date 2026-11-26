//! Vim `p` and `P`: where the register text goes in the Document text.

use std::ops::Range;

use super::VimCommand;
use super::motion::{append_offset, first_non_blank, line_end};
use super::operator::replaced;
use crate::caret::{line_start, previous_boundary};
use crate::{Caret, CaretEdit, EditKind};

/// A put that waits for the register text. Text that ends with a newline is whole lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Put {
    place: Place,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    /// After the char at the offset, or below its line.
    After(usize),
    /// Before the char at the offset, or above its line.
    Before(usize),
    /// In place of a Visual mode selection; `lines` when it is whole lines.
    Over { range: Range<usize>, lines: bool },
}

impl Put {
    pub(super) fn after(offset: usize, count: usize) -> Self {
        Self {
            place: Place::After(offset),
            count,
        }
    }

    pub(super) fn before(offset: usize, count: usize) -> Self {
        Self {
            place: Place::Before(offset),
            count,
        }
    }

    pub(super) fn over(range: Range<usize>, lines: bool, count: usize) -> Self {
        Self {
            place: Place::Over { range, lines },
            count,
        }
    }

    /// The command that puts `register` (the register text) in `text`, as one edit. Chars
    /// go in at the caret and the caret goes on their last char. Lines go in below or
    /// above the line and the caret goes on the first non-blank of the first new line. A put
    /// over a selection gives the selected text to the register. An empty register puts
    /// nothing.
    #[must_use]
    pub fn command(&self, register: &str, text: &str) -> VimCommand {
        if register.is_empty() {
            return VimCommand::Nothing;
        }
        let lines = register.ends_with('\n');
        let body = register.repeat(self.count);
        match &self.place {
            Place::After(offset) if lines => lines_after(text, *offset, &body),
            Place::After(offset) => chars_at(text, append_offset(text, *offset), &body),
            Place::Before(offset) if lines => lines_at(text, line_start(text, *offset), &body),
            Place::Before(offset) => chars_at(text, *offset, &body),
            Place::Over {
                range,
                lines: over_lines,
            } => over(text, range, *over_lines, &body),
        }
    }
}

fn chars_at(text: &str, at: usize, body: &str) -> VimCommand {
    inserted(text, at, body, |after| {
        previous_boundary(after, at + body.len()).max(at)
    })
}

fn lines_at(text: &str, at: usize, body: &str) -> VimCommand {
    inserted(text, at, body, |after| first_non_blank(after, at))
}

/// `body` in at `at`; `caret` gives the caret offset in the text after the edit.
fn inserted(text: &str, at: usize, body: &str, caret: impl FnOnce(&str) -> usize) -> VimCommand {
    let after = replaced(text, &(at..at), body);
    let mut edit = CaretEdit::new(at..at, body, EditKind::Other);
    edit.caret = Caret::at(caret(&after));
    VimCommand::Edit(edit)
}

/// Lines below the line with `offset`. Below the last line they take the newline before them.
fn lines_after(text: &str, offset: usize, body: &str) -> VimCommand {
    let end = line_end(text, offset);
    if end < text.len() {
        return lines_at(text, end + 1, body);
    }
    let body = format!("\n{}", body.strip_suffix('\n').unwrap_or(body));
    let at = text.len();
    inserted(text, at, &body, |after| first_non_blank(after, at + 1))
}

/// `body` in place of `range`. Over whole lines `body` is lines; lines over chars go
/// on lines of their own.
fn over(text: &str, range: &Range<usize>, over_lines: bool, body: &str) -> VimCommand {
    let lines = body.strip_suffix('\n');
    let new = match (over_lines, lines) {
        (true, Some(lines)) => lines.to_owned(),
        (true | false, None) => body.to_owned(),
        (false, Some(lines)) => format!("\n{lines}\n"),
    };
    let removed = text.get(range.clone()).unwrap_or_default();
    let removed = if over_lines {
        format!("{removed}\n")
    } else {
        removed.to_owned()
    };
    let after = replaced(text, range, &new);
    let mut edit = CaretEdit::new(range.clone(), &new, EditKind::Other);
    edit.caret = Caret::at(if over_lines {
        first_non_blank(&after, range.start)
    } else {
        previous_boundary(&after, range.start + new.len()).max(range.start)
    });
    VimCommand::Cut { edit, removed }
}
