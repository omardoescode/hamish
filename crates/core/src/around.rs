//! The sentence and the paragraph around a range of text.

use std::ops::Range;

/// The lines that hold `range` of `text`, without the newlines around them.
#[must_use]
pub fn paragraph_around(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = text
        .get(..range.start)
        .and_then(|before| before.rfind('\n'))
        .map_or(0, |newline| newline + 1);
    let end = text
        .get(range.end..)
        .and_then(|after| after.find('\n'))
        .map_or(text.len(), |newline| range.end + newline);
    start..end
}

/// The sentence of `text` that holds `range`: from after the sentence end before it to the
/// next sentence end, inside its paragraph.
#[must_use]
pub fn sentence_around(text: &str, range: &Range<usize>) -> Range<usize> {
    let paragraph = paragraph_around(text, range);
    let is_end = |c: char| matches!(c, '.' | '!' | '?');
    let before = text.get(paragraph.start..range.start).unwrap_or_default();
    let start = before
        .rfind(is_end)
        .map_or(paragraph.start, |end| paragraph.start + end + 1);
    let start = start
        + text
            .get(start..range.start)
            .map_or(0, |gap| gap.len() - gap.trim_start().len());
    let ends_sentence = text
        .get(..range.end)
        .is_some_and(|head| head.ends_with(is_end));
    if ends_sentence {
        return start..range.end;
    }
    let after = text.get(range.end..paragraph.end).unwrap_or_default();
    let end = after
        .find(is_end)
        .map_or(paragraph.end, |end| range.end + end + 1);
    start..end
}

#[cfg(test)]
mod tests {
    use super::sentence_around;

    const TEXT: &str = "Title\nThe customer waits. She leaves.\nNext one.";

    #[test]
    fn the_sentence_around_a_word_runs_from_the_last_sentence_end_to_the_next() {
        assert_eq!(sentence_around(TEXT, &(10..18)), 6..25);
        assert_eq!(sentence_around(TEXT, &(30..36)), 26..37);
        assert_eq!(sentence_around(TEXT, &(6..25)), 6..25);
        assert_eq!(sentence_around("Title\nno end here", &(9..12)), 6..17);
    }
}
