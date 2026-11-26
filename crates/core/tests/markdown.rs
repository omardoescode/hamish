#![expect(
    clippy::single_range_in_vec_init,
    reason = "a list of one Ghost range is the expected value"
)]

use hamish_core::{Document, Level, MarkdownSpan, MarkdownStyle, Provenance};

fn styles(text: &str) -> Vec<(std::ops::Range<usize>, MarkdownStyle)> {
    styles_of(&Document::new(text))
}

fn styles_of(document: &Document) -> Vec<(std::ops::Range<usize>, MarkdownStyle)> {
    document
        .markdown_styles()
        .into_iter()
        .map(|MarkdownSpan { range, style }| (range, style))
        .collect()
}

#[test]
fn headings_one_to_three_cover_their_line_without_the_newline() {
    assert_eq!(
        styles("# Title\n## Part\n### Step\nBody"),
        [
            (0..7, MarkdownStyle::Heading1),
            (8..15, MarkdownStyle::Heading2),
            (16..24, MarkdownStyle::Heading3),
        ]
    );
}

#[test]
fn bold_italic_and_inline_code_cover_their_marks() {
    assert_eq!(
        styles("A **bold** and *soft* or _also_ with `code`."),
        [
            (2..10, MarkdownStyle::Bold),
            (15..21, MarkdownStyle::Italic),
            (25..31, MarkdownStyle::Italic),
            (37..43, MarkdownStyle::Code),
        ]
    );
}

#[test]
fn italic_inside_bold_gives_both_styles() {
    assert_eq!(
        styles("**very *much* so**"),
        [(0..18, MarkdownStyle::Bold), (7..13, MarkdownStyle::Italic)]
    );
}

#[test]
fn a_block_quote_covers_its_lines() {
    assert_eq!(
        styles("Intro\n\n> said once\n> and again\n\nAfter"),
        [(7..30, MarkdownStyle::Quote)]
    );
}

#[test]
fn list_items_mark_their_list_marker() {
    assert_eq!(
        styles("- one\n- **two**\n\n1. first\n2) other"),
        [
            (0..1, MarkdownStyle::ListMarker),
            (6..7, MarkdownStyle::ListMarker),
            (8..15, MarkdownStyle::Bold),
            (17..19, MarkdownStyle::ListMarker),
            (26..28, MarkdownStyle::ListMarker),
        ]
    );
}

#[test]
fn links_and_autolinks_cover_their_marks() {
    assert_eq!(
        styles("See [the site](https://a.io) or <https://b.io>."),
        [(4..28, MarkdownStyle::Link), (32..46, MarkdownStyle::Link),]
    );
}

#[test]
fn deeper_headings_and_underlined_headings_stay_body_text() {
    assert_eq!(styles("#### Deep\n\nLine\n---\nPlain #not"), []);
}

#[test]
fn styles_follow_each_edit() {
    let mut document = Document::new("Say hi");
    assert_eq!(document.markdown_styles(), []);
    document.insert(4, "**").unwrap();
    assert_eq!(document.markdown_styles(), []);
    document.insert(8, "**").unwrap();
    assert_eq!(
        document.markdown_styles(),
        [MarkdownSpan {
            range: 4..10,
            style: MarkdownStyle::Bold
        }]
    );
    assert!(document.undo());
    assert_eq!(document.markdown_styles(), []);
}

#[test]
fn styles_read_the_active_variant_text() {
    let mut document = Document::new("A plain word.");
    let id = document
        .create_slot(2..7, Level::Word, "*soft*", Provenance::Writer)
        .unwrap();
    assert_eq!(styles_of(&document), []);
    document.set_active(id, 1).unwrap();
    assert_eq!(styles_of(&document), [(2..8, MarkdownStyle::Italic)]);
}

#[test]
fn ghost_text_keeps_its_markdown_style() {
    let mut document = Document::new("# Title\nBody");
    document.ghost(0..7).unwrap();
    assert_eq!(styles_of(&document), [(0..7, MarkdownStyle::Heading1)]);
    assert_eq!(document.ghost_ranges(), [0..7]);
}

#[test]
fn non_ascii_text_gives_byte_ranges() {
    assert_eq!(styles("Λέξη **καλή**"), [(9..21, MarkdownStyle::Bold)]);
}
