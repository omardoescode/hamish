//! Vim motions over plain text: by graphemes, lines and words.

use unicode_segmentation::UnicodeSegmentation;

use super::{line_end, normal_offset};
use crate::caret::{line_start, next_boundary, previous_boundary};

/// The kind of a grapheme for word moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    Blank,
    Word,
    Punctuation,
}

pub(super) fn left(text: &str, offset: usize, times: usize) -> usize {
    let start = line_start(text, offset);
    let mut at = offset;
    for _ in 0..times {
        if at <= start {
            break;
        }
        at = previous_boundary(text, at);
    }
    at
}

pub(super) fn right(text: &str, offset: usize, times: usize) -> usize {
    let end = line_end(text, offset);
    let mut at = offset;
    for _ in 0..times {
        let next = next_boundary(text, at);
        if next >= end {
            break;
        }
        at = next;
    }
    at
}

/// The start of the line `times` lines below the one with `offset`, at most the last line.
pub(super) fn lines_down(text: &str, offset: usize, times: usize) -> usize {
    let mut start = line_start(text, offset);
    for _ in 0..times {
        let end = line_end(text, start);
        if end >= text.len() {
            break;
        }
        start = end + 1;
    }
    start
}

/// The start of the line `times` lines above the one with `offset`, at least the first line.
pub(super) fn lines_up(text: &str, offset: usize, times: usize) -> usize {
    let mut start = line_start(text, offset);
    for _ in 0..times {
        if start == 0 {
            break;
        }
        start = line_start(text, start - 1);
    }
    start
}

/// The start of line `number` (1 is the first line), at most the last line.
pub(super) fn line_number(text: &str, number: usize) -> usize {
    lines_down(text, 0, number.saturating_sub(1))
}

/// The first char of the line at `start` that is not a space or a tab.
pub(super) fn first_non_blank(text: &str, start: usize) -> usize {
    let line = text.get(start..line_end(text, start)).unwrap_or_default();
    let blanks = line.len() - line.trim_start_matches([' ', '\t']).len();
    normal_offset(text, start + blanks)
}

/// How many graphemes come before `offset` on its line.
pub(super) fn column_of(text: &str, offset: usize) -> usize {
    text.get(line_start(text, offset)..offset)
        .map_or(0, |before| before.graphemes(true).count())
}

/// The grapheme at `column` of the line at `start`, at most its last char.
pub(super) fn at_column(text: &str, start: usize, column: usize) -> usize {
    let end = line_end(text, start);
    let mut at = start;
    for _ in 0..column {
        let next = next_boundary(text, at);
        if next >= end {
            break;
        }
        at = next;
    }
    at
}

pub(super) fn class(text: &str, offset: usize) -> Class {
    match text.get(offset..).and_then(|after| after.chars().next()) {
        None => Class::Blank,
        Some(ch) if ch.is_whitespace() => Class::Blank,
        Some(ch) if ch.is_alphanumeric() || ch == '_' => Class::Word,
        Some(_) => Class::Punctuation,
    }
}

/// `offset` is the start of a line with no chars.
pub(super) fn is_empty_line(text: &str, offset: usize) -> bool {
    let at_newline = text
        .get(offset..)
        .is_some_and(|after| after.starts_with('\n'));
    at_newline && line_start(text, offset) == offset
}

/// `step` `times` from `offset`, then on the last char of the line when it ends on the
/// line end.
pub(super) fn repeat(
    text: &str,
    offset: usize,
    times: usize,
    step: fn(&str, usize) -> usize,
) -> usize {
    normal_offset(text, steps(text, offset, times, step))
}

/// `step` `times` from `offset`. A step that does not move ends the repeat.
pub(super) fn steps(
    text: &str,
    offset: usize,
    times: usize,
    step: fn(&str, usize) -> usize,
) -> usize {
    let mut at = offset;
    for _ in 0..times {
        let next = step(text, at);
        if next == at {
            break;
        }
        at = next;
    }
    at
}

/// `w`: the start of the next word or run of punctuation, or of the next empty line.
pub(super) fn word_start_after(text: &str, offset: usize) -> usize {
    let start = class(text, offset);
    let mut at = offset;
    while start != Class::Blank && at < text.len() && class(text, at) == start {
        at = next_boundary(text, at);
    }
    while at < text.len() && class(text, at) == Class::Blank {
        if at != offset && is_empty_line(text, at) {
            break;
        }
        at = next_boundary(text, at);
    }
    at
}

/// `e`: the last char of the word or run of punctuation that ends after `offset`.
pub(super) fn word_end_after(text: &str, offset: usize) -> usize {
    let mut at = next_boundary(text, offset);
    while at < text.len() && class(text, at) == Class::Blank {
        at = next_boundary(text, at);
    }
    let run = class(text, at);
    loop {
        let next = next_boundary(text, at);
        if next >= text.len() || class(text, next) != run {
            return at;
        }
        at = next;
    }
}

/// `b`: the start of the word or run of punctuation before `offset`, or of an empty line.
pub(super) fn word_start_before(text: &str, offset: usize) -> usize {
    let mut at = previous_boundary(text, offset);
    while at > 0 && class(text, at) == Class::Blank && !is_empty_line(text, at) {
        at = previous_boundary(text, at);
    }
    let run = class(text, at);
    if run == Class::Blank {
        return at;
    }
    while at > 0 {
        let before = previous_boundary(text, at);
        if class(text, before) != run {
            break;
        }
        at = before;
    }
    at
}
