//! Vim text objects: the spans that `iw`, `as`, `ip`, `iv`, `ig` and the like give an operator
//! or a Visual mode selection.

use std::ops::Range;

use super::{Span, VimText, line_end, line_start, next_boundary, previous_boundary};
use crate::sentence_around;

/// The kind of a grapheme for word text objects. A newline is a kind of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Newline,
    Blank,
    Word,
    Punctuation,
}

/// The span of text object `object` at `offset`: the inner one, or with `around` the one
/// with the blank around it. `None` when there is no such object at `offset`.
pub(super) fn text_object(
    text: &VimText<'_>,
    offset: usize,
    around: bool,
    object: char,
    times: usize,
) -> Option<Span> {
    let plain = text.text;
    match object {
        'w' if around => around_word(plain, offset, times).map(Span::Chars),
        'w' => inner_word(plain, offset, times).map(Span::Chars),
        's' => Some(Span::Chars(sentence(plain, offset, around))),
        'p' => Some(paragraph(plain, offset, around, times)),
        'v' if !around => innermost(&text.slots, offset).map(Span::Chars),
        'g' if !around => innermost(&text.ghosts, offset).map(Span::Chars),
        _ => None,
    }
}

fn kind(text: &str, at: usize) -> Option<Kind> {
    let ch = text.get(at..)?.chars().next()?;
    Some(match ch {
        '\n' => Kind::Newline,
        ch if ch.is_whitespace() => Kind::Blank,
        ch if ch.is_alphanumeric() || ch == '_' => Kind::Word,
        _ => Kind::Punctuation,
    })
}

/// The end of the run of one kind at `at`. A newline is a run of one.
fn run_end(text: &str, at: usize) -> usize {
    let run = kind(text, at);
    if matches!(run, None | Some(Kind::Newline)) {
        return next_boundary(text, at);
    }
    let mut end = at;
    while kind(text, end) == run {
        end = next_boundary(text, end);
    }
    end
}

/// The start of the run of one kind at `at`.
fn run_start(text: &str, at: usize) -> usize {
    let run = kind(text, at);
    let mut start = at;
    while start > 0 && run != Some(Kind::Newline) {
        let before = previous_boundary(text, start);
        if kind(text, before) != run {
            break;
        }
        start = before;
    }
    start
}

/// `iw`: the word, run of punctuation or run of blanks at `offset`, and `times - 1` runs more.
fn inner_word(text: &str, offset: usize, times: usize) -> Option<Range<usize>> {
    if matches!(kind(text, offset), None | Some(Kind::Newline)) {
        return None;
    }
    let mut end = run_end(text, offset);
    for _ in 1..times {
        if matches!(kind(text, end), None | Some(Kind::Newline)) {
            break;
        }
        end = run_end(text, end);
    }
    Some(run_start(text, offset)..end)
}

/// `aw`: the word at `offset` with the blanks after it, else with the blanks before it.
/// On blanks: the blanks and the word after them.
fn around_word(text: &str, offset: usize, times: usize) -> Option<Range<usize>> {
    let first = kind(text, offset);
    if matches!(first, None | Some(Kind::Newline)) {
        return None;
    }
    let mut end = offset;
    for _ in 0..times {
        end = word_and_blank(text, end);
    }
    let start = run_start(text, offset);
    let trailing = text
        .get(..end)
        .is_some_and(|before| before.ends_with([' ', '\t']));
    if trailing || first == Some(Kind::Blank) {
        return Some(start..end);
    }
    Some(blank_before(text, start)..end)
}

/// The end of the run at `at` and of the run after it, when one of the two is blanks.
fn word_and_blank(text: &str, at: usize) -> usize {
    let end = run_end(text, at);
    let joins = matches!(
        (kind(text, at), kind(text, end)),
        (Some(Kind::Blank), Some(Kind::Word | Kind::Punctuation))
            | (Some(Kind::Word | Kind::Punctuation), Some(Kind::Blank))
    );
    if joins { run_end(text, end) } else { end }
}

/// The start of the blanks right before `start`, else `start`.
fn blank_before(text: &str, start: usize) -> usize {
    let before = previous_boundary(text, start);
    if start > 0 && kind(text, before) == Some(Kind::Blank) {
        return run_start(text, before);
    }
    start
}

/// `is`: the sentence at `offset`. `as`: with the blanks after it, else the blanks before it.
fn sentence(text: &str, offset: usize, around: bool) -> Range<usize> {
    let inner = sentence_around(text, &(offset..offset));
    if !around {
        return inner;
    }
    let after = text
        .get(inner.end..line_end(text, inner.end))
        .unwrap_or_default();
    let blanks = after.len() - after.trim_start_matches([' ', '\t']).len();
    if blanks > 0 {
        return inner.start..inner.end + blanks;
    }
    blank_before(text, inner.start)..inner.end
}

/// Each line is blank: it holds no char but spaces and tabs.
fn is_blank_line(text: &str, start: usize) -> bool {
    text.get(start..line_end(text, start))
        .is_none_or(|line| line.trim_matches([' ', '\t']).is_empty())
}

/// The start of the line after the line at `start`; `None` on the last line.
fn next_line(text: &str, start: usize) -> Option<usize> {
    let end = line_end(text, start);
    (end < text.len()).then_some(end + 1)
}

/// The start of the last line of the block of lines from `start` that are all blank, or all
/// not blank.
fn block_end(text: &str, start: usize) -> usize {
    let blank = is_blank_line(text, start);
    let mut last = start;
    while let Some(next) = next_line(text, last).filter(|&next| is_blank_line(text, next) == blank)
    {
        last = next;
    }
    last
}

/// The start of the first line of the block of lines that holds the line at `start`.
fn block_start(text: &str, start: usize) -> usize {
    let blank = is_blank_line(text, start);
    let mut first = start;
    while first > 0 {
        let before = line_start(text, first - 1);
        if is_blank_line(text, before) != blank {
            break;
        }
        first = before;
    }
    first
}

/// `ip`: the paragraph (or the blank lines) at `offset`, and `times - 1` blocks of lines more.
/// `ap`: each paragraph with the blank lines after it, else the blank lines before it.
fn paragraph(text: &str, offset: usize, around: bool, times: usize) -> Span {
    let line = line_start(text, offset);
    let mut first = block_start(text, line);
    let blocks = if around { times * 2 } else { times };
    let mut last = block_end(text, line);
    for _ in 1..blocks {
        let Some(next) = next_line(text, last) else {
            break;
        };
        last = block_end(text, next);
    }
    let ends_blank = is_blank_line(text, last);
    if around && !ends_blank && first > 0 && !is_blank_line(text, line) {
        first = block_start(text, line_start(text, first - 1));
    }
    Span::Lines(first, last)
}

/// The smallest of `ranges` that holds the char at `offset`.
fn innermost(ranges: &[Range<usize>], offset: usize) -> Option<Range<usize>> {
    ranges
        .iter()
        .filter(|range| range.start <= offset && offset < range.end)
        .min_by_key(|range| range.len())
        .cloned()
}
