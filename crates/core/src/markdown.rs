//! Markdown styles: the spans the editor draws over the active text.

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};

/// A Markdown style the editor draws over the active text (syntax marks stay visible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownStyle {
    Heading1,
    Heading2,
    Heading3,
    Bold,
    Italic,
    Code,
    Quote,
    /// The marker of a list item: `-`, `*`, `+`, `1.`, or `1)`.
    ListMarker,
}

/// One Markdown style over a byte range of the active text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSpan {
    pub range: Range<usize>,
    pub style: MarkdownStyle,
}

/// `line` with its Markdown syntax marks dropped: the text a reader sees.
pub(crate) fn plain_text(line: &str) -> String {
    Parser::new_ext(line, Options::empty())
        .filter_map(|event| match event {
            Event::Text(text) | Event::Code(text) => Some(text),
            Event::Start(_)
            | Event::End(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_) => None,
        })
        .collect::<Vec<_>>()
        .concat()
}

/// The Markdown styles of `text` (`CommonMark`), in document order, outer before inner.
pub(crate) fn spans(text: &str) -> Vec<MarkdownSpan> {
    Parser::new_ext(text, Options::empty())
        .into_offset_iter()
        .filter_map(|(event, range)| span(text, &event, range))
        .collect()
}

fn span(text: &str, event: &Event<'_>, range: Range<usize>) -> Option<MarkdownSpan> {
    let style = match event {
        Event::Start(Tag::Heading { level, .. }) => heading(text, *level, &range)?,
        Event::Start(Tag::Strong) => MarkdownStyle::Bold,
        Event::Start(Tag::Emphasis) => MarkdownStyle::Italic,
        Event::Code(_) => MarkdownStyle::Code,
        Event::Start(Tag::BlockQuote(_)) => MarkdownStyle::Quote,
        Event::Start(Tag::Item) => {
            return Some(MarkdownSpan {
                range: range.start..range.start + list_marker_len(text, range.start),
                style: MarkdownStyle::ListMarker,
            });
        }
        Event::Start(_)
        | Event::End(_)
        | Event::Text(_)
        | Event::InlineMath(_)
        | Event::DisplayMath(_)
        | Event::Html(_)
        | Event::InlineHtml(_)
        | Event::FootnoteReference(_)
        | Event::SoftBreak
        | Event::HardBreak
        | Event::Rule
        | Event::TaskListMarker(_) => return None,
    };
    let block = text.get(range.clone())?;
    let end = range.start + block.trim_end_matches(['\n', '\r']).len();
    Some(MarkdownSpan {
        range: range.start..end,
        style,
    })
}

/// Only ATX headings (`#` to `###`): a line under a `---` or `===` stays body text.
fn heading(text: &str, level: HeadingLevel, range: &Range<usize>) -> Option<MarkdownStyle> {
    text.get(range.clone())?
        .trim_start()
        .starts_with('#')
        .then_some(())?;
    match level {
        HeadingLevel::H1 => Some(MarkdownStyle::Heading1),
        HeadingLevel::H2 => Some(MarkdownStyle::Heading2),
        HeadingLevel::H3 => Some(MarkdownStyle::Heading3),
        HeadingLevel::H4 | HeadingLevel::H5 | HeadingLevel::H6 => None,
    }
}

/// Byte length of the list marker at `start`: up to the first whitespace.
fn list_marker_len(text: &str, start: usize) -> usize {
    let rest = text.get(start..).unwrap_or_default();
    rest.find(char::is_whitespace).unwrap_or(rest.len())
}
