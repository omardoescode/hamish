//! The text runs of the Editor: styled spans (title, Slot, Ghost, Trim cut, Markdown, dim)
//! and the fade-in of a new Active variant, as GPUI text runs.

use std::ops::Range;

use gpui::{App, Font, FontStyle, FontWeight, Hsla, Pixels, TextRun, Window};
use hamish_core::{Level, MarkdownSpan, MarkdownStyle, Toggle};

use super::{Editor, synthetic_bold};
use crate::{Theme, settings};

/// Text runs and the heading lines with their font size. The title line is bold in the
/// title color unless it is a Markdown heading, which uses the heading style.
/// Ghost text dims, Trim cuts fade, and so does text outside the bright
/// paragraph while the Alternatives panel is open. Slot underlines are pen
/// strokes, not text runs.
pub(super) fn text_runs(
    editor: &Editor,
    window: &Window,
    cx: &App,
) -> (Vec<TextRun>, Vec<(usize, Pixels)>) {
    let theme = cx.global::<Theme>();
    let style = window.text_style();
    let shown = editor.shown_document();
    let text = shown.text();
    let markdown = if settings::is_on(cx, Toggle::MarkdownStyles) {
        shown.markdown_styles()
    } else {
        Vec::new()
    };
    let bold_face = synthetic_bold::has_bold_face(&style.font(), window);
    let headings = heading_lines(text, &markdown);
    let [large, medium, small] = theme.font_size_heading;
    let sized_lines = headings
        .iter()
        .map(|&(start, level)| {
            let size = match level {
                MarkdownStyle::Heading1 => large,
                MarkdownStyle::Heading2 => medium,
                MarkdownStyle::Heading3
                | MarkdownStyle::Bold
                | MarkdownStyle::Italic
                | MarkdownStyle::Code
                | MarkdownStyle::Quote
                | MarkdownStyle::ListMarker
                | MarkdownStyle::Link => small,
            };
            (start, size)
        })
        .collect();
    let title_is_heading = headings.first().is_some_and(|&(start, _)| start == 0);
    let title_end = if title_is_heading {
        0
    } else {
        shown.title().map_or(0, str::len)
    };
    let underlined: Vec<Range<usize>> = shown
        .slots()
        .into_iter()
        .filter(|slot| slot.level != Level::Paragraph)
        .map(|slot| slot.range)
        .collect();
    let runs: Vec<TextRun> = spans(
        text.len(),
        title_end,
        &underlined,
        &shown.ghost_ranges(),
        &editor.trim_fades(),
        &markdown,
        editor.bright_paragraph().as_ref(),
    )
    .into_iter()
    .map(|span| {
        let mut font = style.font();
        let mut color = theme.text;
        if span.title {
            font.weight = FontWeight::BOLD;
            color = theme.text_title;
        }
        let (font, color, background) =
            markdown_look(&span.markdown, theme, bold_face, font, color);
        let opacity = span.opacity(theme);
        TextRun {
            len: span.range.len(),
            font,
            color: color.opacity(opacity),
            background_color: background.map(|background| background.opacity(opacity)),
            underline: None,
            strikethrough: None,
        }
    })
    .collect();
    let fading = editor
        .fading_in(cx)
        .and_then(|(id, opacity)| editor.document().slot(id).map(|slot| (slot.range, opacity)));
    let runs = match fading {
        Some((range, opacity)) => faded_runs(&runs, &range, opacity),
        None => runs,
    };
    if runs.is_empty() {
        let empty = TextRun {
            len: 0,
            font: style.font(),
            color: theme.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        return (vec![empty], sized_lines);
    }
    (runs, sized_lines)
}

/// A stretch of the text with one style.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one independent style layer of the span"
)]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) range: Range<usize>,
    title: bool,
    pub(super) underlined: bool,
    ghost: bool,
    /// The Markdown styles over it, outer first.
    markdown: Vec<MarkdownStyle>,
    /// Outside the bright paragraph while the Alternatives panel is open.
    dimmed: bool,
    /// In a cut of the Trim proposal.
    faded: bool,
}

/// Splits text of `len` bytes at the title end, at each underlined, Ghost, Trim cut, and Markdown
/// range edge, and at the edges of the `bright` range, outside of which text is dimmed.
pub(super) fn spans(
    len: usize,
    title_end: usize,
    underlined: &[Range<usize>],
    ghosts: &[Range<usize>],
    cuts: &[Range<usize>],
    markdown: &[MarkdownSpan],
    bright: Option<&Range<usize>>,
) -> Vec<Span> {
    let mut edges = vec![0, title_end, len];
    edges.extend(bright.iter().flat_map(|range| [range.start, range.end]));
    edges.extend(
        underlined
            .iter()
            .chain(ghosts)
            .chain(cuts)
            .chain(markdown.iter().map(|span| &span.range))
            .flat_map(|range| [range.start, range.end]),
    );
    edges.sort_unstable();
    edges.dedup();
    let count = edges.len().saturating_sub(1);
    // Span i runs from edges[i] to edges[i + 1]; each range edge is an edge, so a range
    // holds a run of whole spans.
    let spans_in = |range: &Range<usize>| {
        edges.partition_point(|&edge| edge < range.start)
            ..edges.partition_point(|&edge| edge < range.end)
    };
    let within = |ranges: &[Range<usize>]| {
        let mut inside = vec![false; count];
        for index in ranges.iter().flat_map(spans_in) {
            if let Some(flag) = inside.get_mut(index) {
                *flag = true;
            }
        }
        inside
    };
    let (underlined, ghost, faded) = (within(underlined), within(ghosts), within(cuts));
    let bright = bright.map(|bright| within(std::slice::from_ref(bright)));
    let mut styles: Vec<Vec<MarkdownStyle>> = vec![Vec::new(); count];
    for span in markdown {
        for index in spans_in(&span.range) {
            if let Some(over) = styles.get_mut(index) {
                over.push(span.style);
            }
        }
    }
    edges
        .iter()
        .zip(edges.iter().skip(1))
        .zip(styles)
        .enumerate()
        .map(|(index, ((&start, &end), markdown))| {
            let flag = |flags: &[bool]| flags.get(index).copied().unwrap_or_default();
            Span {
                range: start..end,
                title: end <= title_end,
                underlined: flag(&underlined),
                ghost: flag(&ghost),
                markdown,
                faded: flag(&faded),
                dimmed: bright.as_ref().is_some_and(|bright| !flag(bright)),
            }
        })
        .collect()
}

impl Span {
    /// Ghost, dim, and Trim fade opacity of the text and its underline.
    pub(super) fn opacity(&self, theme: &Theme) -> f32 {
        let ghost = if self.ghost { theme.ghost_opacity } else { 1. };
        let dim = if self.dimmed {
            theme.panel_open_dim_opacity
        } else {
            1.
        };
        let fade = if self.faded {
            theme.trim_fade_opacity
        } else {
            1.
        };
        ghost * dim * fade
    }
}

/// Font, color, and background of a run after the Markdown styles over it, outer first.
/// With no `bold_face`, a heading shows in the accent color and normal weight, because
/// synthetic bold smears large text.
fn markdown_look(
    styles: &[MarkdownStyle],
    theme: &Theme,
    bold_face: bool,
    mut font: Font,
    mut color: Hsla,
) -> (Font, Hsla, Option<Hsla>) {
    let mut background = None;
    for style in styles {
        match style {
            MarkdownStyle::Heading1 | MarkdownStyle::Heading2 | MarkdownStyle::Heading3
                if bold_face =>
            {
                font.weight = FontWeight::BOLD;
                color = theme.text_heading;
            }
            MarkdownStyle::Heading1 | MarkdownStyle::Heading2 | MarkdownStyle::Heading3 => {
                color = theme.accent;
            }
            MarkdownStyle::Bold => font.weight = FontWeight::BOLD,
            MarkdownStyle::Italic => font.style = FontStyle::Italic,
            MarkdownStyle::Code => background = Some(theme.code_background),
            MarkdownStyle::Quote => color = theme.text_quote,
            MarkdownStyle::ListMarker => color = theme.list_marker,
            MarkdownStyle::Link => color = theme.accent,
        }
    }
    (font, color, background)
}

/// The start of each line that holds a Markdown heading, with the heading style.
fn heading_lines(text: &str, markdown: &[MarkdownSpan]) -> Vec<(usize, MarkdownStyle)> {
    markdown
        .iter()
        .filter(|span| {
            matches!(
                span.style,
                MarkdownStyle::Heading1 | MarkdownStyle::Heading2 | MarkdownStyle::Heading3
            )
        })
        .map(|span| {
            let before = text.get(..span.range.start).unwrap_or_default();
            let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
            (line_start, span.style)
        })
        .collect()
}

/// `runs` (which cover the text from offset 0) with the part inside byte `range` at
/// `opacity`.
fn faded_runs(runs: &[TextRun], range: &Range<usize>, opacity: f32) -> Vec<TextRun> {
    let mut at = 0;
    let mut faded = Vec::with_capacity(runs.len() + 2);
    for run in runs {
        let start = at;
        at += run.len;
        let inside = start.max(range.start)..at.min(range.end);
        if inside.is_empty() {
            faded.push(run.clone());
            continue;
        }
        let parts = [
            (start..inside.start, 1.),
            (inside.clone(), opacity),
            (inside.end..at, 1.),
        ];
        faded.extend(parts.into_iter().filter(|(part, _)| !part.is_empty()).map(
            |(part, factor)| TextRun {
                len: part.len(),
                color: run.color.opacity(factor),
                background_color: run.background_color.map(|color| color.opacity(factor)),
                ..run.clone()
            },
        ));
    }
    faded
}

#[cfg(test)]
mod tests {
    use hamish_core::{MarkdownSpan, MarkdownStyle};

    use gpui::{TextRun, font, hsla};

    use super::{Span, faded_runs, heading_lines, spans};

    #[test]
    fn a_fade_splits_the_runs_at_its_range_and_dims_only_inside() {
        let run = |len| TextRun {
            len,
            font: font("IBM Plex Mono"),
            color: hsla(0., 0., 1., 1.),
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let faded = faded_runs(&[run(4), run(6)], &(2..7), 0.5);

        let shape: Vec<(usize, f32)> = faded.iter().map(|run| (run.len, run.color.a)).collect();
        assert_eq!(shape, [(2, 1.), (2, 0.5), (3, 0.5), (3, 1.)]);
    }

    #[test]
    fn ghost_and_slot_ranges_split_the_text_into_styled_spans() {
        let span = |range, title, underlined, ghost| Span {
            range,
            title,
            underlined,
            ghost,
            markdown: Vec::new(),
            dimmed: false,
            faded: false,
        };
        assert_eq!(
            spans(
                27,
                5,
                std::slice::from_ref(&(14..21)),
                std::slice::from_ref(&(6..13)),
                &[],
                &[],
                None
            ),
            vec![
                span(0..5, true, false, false),
                span(5..6, false, false, false),
                span(6..13, false, false, true),
                span(13..14, false, false, false),
                span(14..21, false, true, false),
                span(21..27, false, false, false),
            ]
        );
    }

    #[test]
    fn markdown_styles_split_spans_and_nest() {
        let markdown = |range, style| MarkdownSpan { range, style };
        let span = |range, markdown| Span {
            range,
            title: false,
            underlined: false,
            ghost: false,
            markdown,
            dimmed: false,
            faded: false,
        };
        assert_eq!(
            spans(
                12,
                0,
                &[],
                &[],
                &[],
                &[
                    markdown(0..12, MarkdownStyle::Bold),
                    markdown(2..5, MarkdownStyle::Italic)
                ],
                None
            ),
            vec![
                span(0..2, vec![MarkdownStyle::Bold]),
                span(2..5, vec![MarkdownStyle::Bold, MarkdownStyle::Italic]),
                span(5..12, vec![MarkdownStyle::Bold]),
            ]
        );
    }

    #[test]
    fn heading_lines_start_at_the_line_of_each_heading() {
        let text = "# Top\nbody\n> ## Quoted\n### Low";
        let styles = hamish_core::Document::new(text).markdown_styles();
        assert_eq!(
            heading_lines(text, &styles),
            vec![
                (0, MarkdownStyle::Heading1),
                (11, MarkdownStyle::Heading2),
                (23, MarkdownStyle::Heading3)
            ]
        );
    }

    #[test]
    fn text_outside_the_bright_range_is_dimmed() {
        let dimmed: Vec<(std::ops::Range<usize>, bool)> =
            spans(30, 5, &[], &[], &[], &[], Some(&(6..18)))
                .into_iter()
                .map(|span| (span.range, span.dimmed))
                .collect();
        assert_eq!(
            dimmed,
            vec![(0..5, true), (5..6, true), (6..18, false), (18..30, true)]
        );
    }

    #[test]
    fn trim_cuts_fade_apart_from_ghost_ranges() {
        let faded: Vec<(std::ops::Range<usize>, bool, bool)> = spans(
            20,
            0,
            &[],
            std::slice::from_ref(&(8..12)),
            &[2..5, 10..15],
            &[],
            None,
        )
        .into_iter()
        .map(|span| (span.range, span.ghost, span.faded))
        .collect();
        assert_eq!(
            faded,
            vec![
                (0..2, false, false),
                (2..5, false, true),
                (5..8, false, false),
                (8..10, true, false),
                (10..12, true, true),
                (12..15, false, true),
                (15..20, false, false),
            ]
        );
    }
}
