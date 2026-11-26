//! Synthetic bold for a font with no bold face. GPUI on Linux uses no font variation axes,
//! so a variable font (Adwaita Sans, say) has no bold; its bold glyphs paint twice.

use std::ops::Range;

use gpui::{Font, FontId, FontWeight, GlyphId, Hsla, Pixels, Point, TextRun, Window, point, px};

use super::painted_text::{PaintedText, run_ends};

/// The second paint is this part of the font size to the right of the first.
const SPREAD: f32 = 1. / 24.;

/// Where the second paint of a glyph at `font_size` goes, from the first.
pub(super) fn spread(font_size: Pixels) -> Point<Pixels> {
    point(font_size * SPREAD, px(0.))
}

/// Byte ranges of `runs` (which cover the text from offset 0) that ask for bold in a font
/// whose bold resolves to its regular face.
pub(super) fn ranges(runs: &[TextRun], window: &Window) -> Vec<Range<usize>> {
    let text_system = window.text_system();
    let mut start = 0;
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for run in runs {
        let range = start..start + run.len;
        start = range.end;
        let regular = Font {
            weight: FontWeight::NORMAL,
            ..run.font.clone()
        };
        let no_bold_face = run.font.weight >= FontWeight::SEMIBOLD
            && text_system.resolve_font(&run.font) == text_system.resolve_font(&regular);
        if !no_bold_face {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => ranges.push(range),
        }
    }
    ranges
}

/// The second paint of one glyph (window coordinates).
pub(super) struct BoldGlyph {
    /// Index in the painted lines.
    pub(super) line: usize,
    /// Byte offset in the text.
    pub(super) offset: usize,
    origin: Point<Pixels>,
    font_id: FontId,
    id: GlyphId,
    font_size: Pixels,
    color: Hsla,
}

impl BoldGlyph {
    /// A failed glyph paint leaves the glyph regular; the next frame tries again.
    pub(super) fn paint(&self, window: &mut Window) {
        window
            .paint_glyph(
                self.origin + spread(self.font_size),
                self.font_id,
                self.id,
                self.font_size,
                self.color,
            )
            .ok();
    }
}

/// The glyphs of `painted` inside `bold` (sorted, disjoint), with the colors of `runs`.
pub(super) fn glyphs(
    painted: &PaintedText,
    runs: &[TextRun],
    bold: &[Range<usize>],
) -> Vec<BoldGlyph> {
    if bold.is_empty() {
        return Vec::new();
    }
    let first_ending_after = |offset: usize| bold.partition_point(|range| range.end <= offset);
    let run_ends = run_ends(runs);
    let color_at = |offset: usize| {
        runs.get(run_ends.partition_point(|&end| end <= offset))
            .map_or_else(Hsla::default, |run| run.color)
    };
    painted
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let range = line.range();
            bold.get(first_ending_after(range.start))
                .is_some_and(|bold| bold.start < range.end)
        })
        .flat_map(|(index, line)| {
            let range = line.range();
            let line_bold = bold
                .get(
                    first_ending_after(range.start)
                        ..bold.partition_point(|bold| bold.start < range.end),
                )
                .unwrap_or_default();
            let in_bold = move |offset: usize| {
                line_bold
                    .get(line_bold.partition_point(|range| range.end <= offset))
                    .is_some_and(|range| range.contains(&offset))
            };
            let origin = painted.bounds.origin + point(px(0.), line.top);
            let font_size = line.line.unwrapped_layout.font_size;
            line.glyphs(origin, painted.line_height)
                .into_iter()
                .filter(move |glyph| !glyph.emoji && in_bold(glyph.offset))
                .map(move |glyph| BoldGlyph {
                    line: index,
                    offset: glyph.offset,
                    origin: glyph.baseline,
                    font_id: glyph.font_id,
                    id: glyph.id,
                    font_size,
                    color: color_at(glyph.offset),
                })
        })
        .collect()
}
