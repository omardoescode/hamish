//! The caret and the selection in one text: grapheme moves, edits at the caret, and the
//! UTF-16 ranges of input methods. Pixels stay in the app.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::EditKind;

/// The selection in one text, its caret end, and the text an input method composes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Caret {
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
}

/// Where a caret move goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// One grapheme back.
    Left,
    /// One grapheme on.
    Right,
    /// To the start of the word before.
    WordLeft,
    /// To the end of the word after.
    WordRight,
    /// To the start of the line: just after the newline before, else the text start.
    LineStart,
    /// To the end of the line: the newline after, else the text end.
    LineEnd,
    /// A byte offset the app found, for example from the painted rows or the pointer.
    To(usize),
}

/// One edit a caret action asks for: replace `range` with `text`, then show `caret`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaretEdit {
    /// Byte range of the text to replace.
    pub range: Range<usize>,
    pub text: String,
    /// How the edit joins an undo step.
    pub kind: EditKind,
    /// The caret after the edit.
    pub caret: Caret,
}

impl CaretEdit {
    /// Replaces `range` with `text`; the caret goes after the new text.
    #[must_use]
    pub fn new(range: Range<usize>, text: &str, kind: EditKind) -> Self {
        Self {
            caret: Caret::at(range.start + text.len()),
            range,
            text: text.to_owned(),
            kind,
        }
    }
}

impl Caret {
    /// An empty selection at byte `offset`.
    #[must_use]
    pub fn at(offset: usize) -> Self {
        Self {
            selection: offset..offset,
            ..Self::default()
        }
    }

    /// A selection of byte `range`, the caret at its end.
    #[must_use]
    pub fn over(range: Range<usize>) -> Self {
        Self {
            selection: range,
            ..Self::default()
        }
    }

    /// The word at byte `offset` of `text` selected (see [`word_at`]), the caret at its
    /// end; with no word there, the caret at `offset`.
    #[must_use]
    pub fn word(text: &str, offset: usize) -> Self {
        word_at(text, offset).map_or_else(|| Self::at(floor_boundary(text, offset)), Self::over)
    }

    /// The whole of `text` selected, the caret at its end.
    #[must_use]
    pub fn all(text: &str) -> Self {
        Self {
            selection: 0..text.len(),
            ..Self::default()
        }
    }

    /// Byte range of the selection; empty at the caret.
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// Byte offset of the caret end of the selection.
    #[must_use]
    pub fn offset(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    /// This caret moved inside `text` by `motion`; the selection collapses. Left and Right
    /// over a selection go to its start and end.
    #[must_use]
    pub fn moved(&self, text: &str, motion: Motion) -> Self {
        let caret = self.clamped(text);
        let offset = match motion {
            Motion::Left if !caret.selection.is_empty() => caret.selection.start,
            Motion::Right if !caret.selection.is_empty() => caret.selection.end,
            Motion::Left
            | Motion::Right
            | Motion::WordLeft
            | Motion::WordRight
            | Motion::LineStart
            | Motion::LineEnd
            | Motion::To(_) => target(text, caret.offset(), motion),
        };
        Self::at(offset)
    }

    /// The selection with its caret end moved inside `text` by `motion`; the other end
    /// stays.
    #[must_use]
    pub fn extended(&self, text: &str, motion: Motion) -> Self {
        let caret = self.clamped(text);
        let offset = target(text, caret.offset(), motion);
        let anchor = if caret.reversed {
            caret.selection.end
        } else {
            caret.selection.start
        };
        Self {
            selection: anchor.min(offset)..anchor.max(offset),
            reversed: offset < anchor,
            marked: None,
        }
    }

    /// Deletes the selection, or with none the text from the caret to where `motion` goes.
    #[must_use]
    pub fn delete(&self, text: &str, motion: Motion) -> CaretEdit {
        let caret = self.clamped(text);
        if !caret.selection.is_empty() {
            return CaretEdit::new(caret.selection, "", EditKind::Other);
        }
        let to = caret.moved(text, motion).offset();
        let from = caret.offset();
        CaretEdit::new(from.min(to)..from.max(to), "", EditKind::Deleting)
    }

    /// Puts `new` in place of the selection, as an edit that joins no other.
    #[must_use]
    pub fn insert(&self, text: &str, new: &str) -> CaretEdit {
        CaretEdit::new(self.clamped(text).selection, new, EditKind::Other)
    }

    /// Puts `new` from an input method in place of `range_utf16`, else the marked text,
    /// else the selection. At the caret it is typing; over a range it joins no other edit.
    #[must_use]
    pub fn input(&self, text: &str, range_utf16: Option<Range<usize>>, new: &str) -> CaretEdit {
        let range = self.input_range(text, range_utf16);
        let kind = if range.is_empty() {
            EditKind::Typing
        } else {
            EditKind::Other
        };
        CaretEdit::new(range, new, kind)
    }

    /// Like [`Self::input`], but `new` stays marked as composed text, and
    /// `selected_utf16` (counted inside `new`) is the selection after it. With no
    /// `selected_utf16` the caret goes after `new`.
    #[must_use]
    pub fn compose(
        &self,
        text: &str,
        range_utf16: Option<Range<usize>>,
        new: &str,
        selected_utf16: Option<Range<usize>>,
    ) -> CaretEdit {
        let range = self.input_range(text, range_utf16);
        let mut edit = CaretEdit::new(range.clone(), new, EditKind::Typing);
        if let Some(selected) = selected_utf16 {
            let selected = byte_range(new, &selected);
            edit.caret.selection = range.start + selected.start..range.start + selected.end;
        }
        edit.caret.marked = (!new.is_empty()).then(|| range.start..range.start + new.len());
        edit
    }

    /// The selection in UTF-16 units of `text`.
    #[must_use]
    pub fn selected_utf16(&self, text: &str) -> Range<usize> {
        utf16_range(text, &self.selection)
    }

    /// The caret is at the start of the selection.
    #[must_use]
    pub fn is_reversed(&self) -> bool {
        self.reversed
    }

    /// Byte range of the text an input method composes; `None` when it composes none.
    #[must_use]
    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    /// The composed text in UTF-16 units of `text`.
    #[must_use]
    pub fn marked_utf16(&self, text: &str) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| utf16_range(text, range))
    }

    /// This caret with no composed text.
    #[must_use]
    pub fn unmarked(&self) -> Self {
        Self {
            marked: None,
            ..self.clone()
        }
    }

    fn input_range(&self, text: &str, range_utf16: Option<Range<usize>>) -> Range<usize> {
        let caret = self.clamped(text);
        range_utf16
            .map(|range| byte_range(text, &range))
            .or(caret.marked)
            .unwrap_or(caret.selection)
    }

    /// This selection moved along with one changed span of `old` into `new` (see
    /// [`follow_change`]); the composed text ends.
    #[must_use]
    pub fn followed(&self, old: &str, new: &str) -> Self {
        Self {
            selection: follow_change(self.selection.start, old, new)
                ..follow_change(self.selection.end, old, new),
            reversed: self.reversed,
            marked: None,
        }
    }

    /// This caret with each offset moved back to a char boundary of `text`, at most its
    /// length. The text can change under a caret that is not its own.
    #[must_use]
    pub fn clamped(&self, text: &str) -> Self {
        let fit = |range: &Range<usize>| {
            floor_boundary(text, range.start)..floor_boundary(text, range.end)
        };
        Self {
            selection: fit(&self.selection),
            reversed: self.reversed,
            marked: self.marked.as_ref().map(fit),
        }
    }
}

/// Where `motion` goes from `offset` in `text`, as if nothing is selected.
fn target(text: &str, offset: usize, motion: Motion) -> usize {
    match motion {
        Motion::Left => previous_boundary(text, offset),
        Motion::Right => next_boundary(text, offset),
        Motion::WordLeft => words(text)
            .filter(|word| word.start < offset)
            .last()
            .map_or(0, |word| word.start),
        Motion::WordRight => words(text)
            .find(|word| word.end > offset)
            .map_or(text.len(), |word| word.end),
        Motion::LineStart => text
            .get(..offset)
            .and_then(|before| before.rfind('\n'))
            .map_or(0, |newline| newline + 1),
        Motion::LineEnd => text
            .get(offset..)
            .and_then(|after| after.find('\n'))
            .map_or(text.len(), |newline| offset + newline),
        Motion::To(offset) => floor_boundary(text, offset),
    }
}

/// The byte ranges of the words of `text`: word-bound runs with a letter or digit.
fn words(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    text.split_word_bound_indices()
        .filter(|(_, word)| word.chars().any(char::is_alphanumeric))
        .map(|(start, word)| start..start + word.len())
}

/// The word at byte `offset` of `text`: the word `offset` lies in or on an edge of.
#[must_use]
pub fn word_at(text: &str, offset: usize) -> Option<Range<usize>> {
    words(text).find(|word| word.start <= offset && offset <= word.end)
}

fn floor_boundary(text: &str, offset: usize) -> usize {
    (0..=offset.min(text.len()))
        .rev()
        .find(|&at| text.is_char_boundary(at))
        .unwrap_or(0)
}

fn previous_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(index, _)| (index < offset).then_some(index))
        .unwrap_or(0)
}

fn next_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(index, _)| (index > offset).then_some(index))
        .unwrap_or(text.len())
}

/// Byte range `bytes` of `text` in UTF-16 units. A byte offset inside a char counts from
/// the start of that char.
#[must_use]
pub fn utf16_range(text: &str, bytes: &Range<usize>) -> Range<usize> {
    let units = |offset: usize| {
        text.get(..floor_boundary(text, offset))
            .map_or(0, |before| before.encode_utf16().count())
    };
    units(bytes.start)..units(bytes.end)
}

/// UTF-16 range `units` of `text` in bytes. A unit inside a char moves to the end of that
/// char; past the text is its length.
#[must_use]
pub fn byte_range(text: &str, units: &Range<usize>) -> Range<usize> {
    let bytes = |offset: usize| {
        let mut counted = 0;
        text.char_indices()
            .find_map(|(index, ch)| {
                let found = (counted >= offset).then_some(index);
                counted += ch.len_utf16();
                found
            })
            .unwrap_or(text.len())
    };
    bytes(units.start)..bytes(units.end)
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
