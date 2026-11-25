//! The text as last painted: its wrapped lines and their geometry (offsets, rows, quads).
//! The Editor and Overflow shape, paint and hit-test their text through it.

use std::ops::Range;

use gpui::{
    App, Bounds, FontId, GlyphId, Hsla, PaintQuad, Pixels, Point, SharedString, TextRun, Window,
    WrappedLine, fill, point, px,
};

/// The text as last painted, for mouse and vertical caret moves.
pub(crate) struct PaintedText {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) line_height: Pixels,
    pub(crate) lines: Vec<PaintedLine>,
}

pub(crate) struct PaintedLine {
    pub(super) start: usize,
    pub(super) top: Pixels,
    pub(super) line: WrappedLine,
}

/// One glyph of a painted line, before any zoom (window coordinates).
#[derive(Debug, Clone, Copy)]
pub(super) struct LineGlyph {
    /// Byte offset in the text.
    pub(super) offset: usize,
    /// Top left of the glyph cell on its row.
    pub(super) top_left: Point<Pixels>,
    pub(super) baseline: Point<Pixels>,
    /// Advance to the next glyph, or to the line end.
    pub(super) width: Pixels,
    pub(super) font_id: FontId,
    pub(super) id: GlyphId,
    pub(super) emoji: bool,
}

impl PaintedLine {
    /// The glyphs of this line, its top left at `origin`, each on its wrapped row.
    pub(super) fn glyphs(&self, origin: Point<Pixels>, line_height: Pixels) -> Vec<LineGlyph> {
        let layout = &self.line.unwrapped_layout;
        let baseline = (line_height - layout.ascent - layout.descent) / 2. + layout.ascent;
        let positioned: Vec<_> = layout
            .runs
            .iter()
            .enumerate()
            .flat_map(|(run_ix, run)| {
                run.glyphs
                    .iter()
                    .enumerate()
                    .map(move |(glyph_ix, glyph)| (run_ix, glyph_ix, run.font_id, glyph))
            })
            .collect();
        let mut row = 0_u16;
        let mut row_start = px(0.);
        let mut glyphs = Vec::with_capacity(positioned.len());
        for (index, &(run_ix, glyph_ix, font_id, glyph)) in positioned.iter().enumerate() {
            if self
                .line
                .wrap_boundaries
                .iter()
                .any(|wrap| wrap.run_ix == run_ix && wrap.glyph_ix == glyph_ix)
            {
                row += 1;
                row_start = glyph.position.x;
            }
            let top_left =
                origin + point(glyph.position.x - row_start, line_height * f32::from(row));
            let next_x = positioned
                .get(index + 1)
                .map_or(layout.width, |next| next.3.position.x);
            glyphs.push(LineGlyph {
                offset: self.start + glyph.index,
                top_left,
                baseline: top_left + point(px(0.), baseline),
                width: next_x - glyph.position.x,
                font_id,
                id: glyph.id,
                emoji: glyph.is_emoji,
            });
        }
        glyphs
    }

    /// Byte range of this line in the text.
    pub(super) fn range(&self) -> Range<usize> {
        self.start..self.start + self.line.len()
    }
}

/// The run that covers byte `offset`.
pub(super) fn run_at(runs: &[TextRun], offset: usize) -> Option<&TextRun> {
    let mut end = 0;
    runs.iter().find(|run| {
        end += run.len;
        offset < end
    })
}

impl PaintedText {
    /// The last line for which `holds` is true. The lines are in text order, so their starts
    /// and tops only grow: `holds` must be true for a prefix of the lines.
    fn last_line_where(&self, holds: impl Fn(&PaintedLine) -> bool) -> Option<&PaintedLine> {
        let count = self.lines.partition_point(holds);
        self.lines.get(count.checked_sub(1)?)
    }

    /// Height of all lines.
    pub(crate) fn height(&self) -> Pixels {
        self.lines.last().map_or(self.line_height, |last| {
            last.top + last.line.size(self.line_height).height
        })
    }

    /// Paints the lines from the top left of `bounds`. A failed glyph paint leaves that line
    /// blank; the next frame tries again.
    pub(crate) fn paint(&self, window: &mut Window, cx: &mut App) {
        for painted in &self.lines {
            let origin = self.bounds.origin + point(px(0.), painted.top);
            painted
                .line
                .paint(
                    origin,
                    self.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
        }
    }

    /// Position of `offset`, relative to the text origin.
    pub(crate) fn position_for(&self, offset: usize) -> Option<Point<Pixels>> {
        let painted = self.last_line_where(|line| line.start <= offset)?;
        let position = painted
            .line
            .position_for_index(offset - painted.start, self.line_height)?;
        Some(point(position.x, painted.top + position.y))
    }

    /// Offset closest to `local` (relative to the text origin). Above the text is 0,
    /// below it is `len`.
    pub(crate) fn offset_for_local(&self, local: Point<Pixels>, len: usize) -> usize {
        if local.y < px(0.) {
            return 0;
        }
        let Some(painted) = self.last_line_where(|line| line.top <= local.y) else {
            return 0;
        };
        let height = painted.line.size(self.line_height).height;
        if local.y >= painted.top + height {
            return len;
        }
        let row_point = point(local.x.max(px(0.)), local.y - painted.top);
        let index = match painted
            .line
            .closest_index_for_position(row_point, self.line_height)
        {
            Ok(index) | Err(index) => index,
        };
        painted.start + index
    }

    /// Offset of the glyph under `local` (relative to the text origin); `None` off the glyphs.
    pub(super) fn glyph_at(&self, local: Point<Pixels>) -> Option<usize> {
        if local.x < px(0.) {
            return None;
        }
        let painted = self.last_line_where(|line| line.top <= local.y)?;
        let row_point = point(local.x, local.y - painted.top);
        let index = painted
            .line
            .index_for_position(row_point, self.line_height)
            .ok()?;
        Some(painted.start + index)
    }

    /// Rows from the row of `range.start` to the row of `range.end` (local tops).
    pub(super) fn rows(&self, range: &Range<usize>) -> Option<(Point<Pixels>, Point<Pixels>)> {
        Some((
            self.position_for(range.start)?,
            self.position_for(range.end)?,
        ))
    }

    /// Local y of the underline on the row at `top`: where gpui puts a text underline.
    pub(super) fn underline_y(&self, top: Pixels) -> Pixels {
        let Some(painted) = self.last_line_where(|line| line.top <= top) else {
            return top + self.line_height;
        };
        let (ascent, descent) = (painted.line.ascent(), painted.line.descent());
        top + (self.line_height - ascent - descent) / 2. + ascent + descent * 0.618
    }

    /// Quads that cover `range` (window coordinates), one per visual row. A wrapped row
    /// fills to the right edge.
    pub(crate) fn selection_quads(&self, range: &Range<usize>, color: Hsla) -> Vec<PaintQuad> {
        let width = self.bounds.size.width;
        self.row_quads(range, color, |_| width)
    }

    /// Quads that cover `range` (window coordinates), one per visual row. Each row ends
    /// at the end of its text, not at the right edge.
    pub(crate) fn text_quads(&self, range: &Range<usize>, color: Hsla) -> Vec<PaintQuad> {
        let width = self.bounds.size.width;
        self.row_quads(range, color, |top| self.text_end(top).unwrap_or(width))
    }

    /// Local x where the text of the row at local `top` ends, less trailing spaces.
    fn text_end(&self, top: Pixels) -> Option<Pixels> {
        let painted = self.last_line_where(|line| line.top <= top)?;
        let layout = &painted.line.unwrapped_layout;
        let wraps = painted.line.wrap_boundaries.iter().filter_map(|boundary| {
            let run = layout.runs.get(boundary.run_ix)?;
            run.glyphs.get(boundary.glyph_ix).map(|glyph| glyph.index)
        });
        let starts = std::iter::once(0).chain(wraps.clone());
        let ends = wraps.chain(std::iter::once(painted.line.len()));
        let mut row_bottom = painted.top + self.line_height;
        for (start, end) in starts.zip(ends) {
            if top < row_bottom {
                let text = painted.line.text.get(start..end)?;
                let text_end = start + text.trim_end().len();
                return Some(layout.x_for_index(text_end) - layout.x_for_index(start));
            }
            row_bottom += self.line_height;
        }
        None
    }

    fn row_quads(
        &self,
        range: &Range<usize>,
        color: Hsla,
        row_right: impl Fn(Pixels) -> Pixels,
    ) -> Vec<PaintQuad> {
        let (Some(start), Some(end)) =
            (self.position_for(range.start), self.position_for(range.end))
        else {
            return Vec::new();
        };
        let origin = self.bounds.origin;
        let mut quads = Vec::new();
        let mut top = start.y;
        while top <= end.y {
            let left = if top == start.y { start.x } else { px(0.) };
            let right = if top == end.y {
                end.x
            } else {
                row_right(top).max(left)
            };
            quads.push(fill(
                Bounds::from_corners(
                    origin + point(left, top),
                    origin + point(right, top + self.line_height),
                ),
                color,
            ));
            top += self.line_height;
        }
        quads
    }
}

/// Shapes `text` into wrapped lines, each with its start offset and top.
pub(crate) fn shape(
    text: &SharedString,
    runs: &[TextRun],
    wrap_width: Option<Pixels>,
    line_height: Pixels,
    window: &Window,
) -> Vec<PaintedLine> {
    shape_sized(text, runs, &[], wrap_width, line_height, window)
}

/// Like [`shape`], but each line that starts at an offset in `sized_lines` takes that font
/// size in place of the body size. All lines keep `line_height`.
pub(super) fn shape_sized(
    text: &SharedString,
    runs: &[TextRun],
    sized_lines: &[(usize, Pixels)],
    wrap_width: Option<Pixels>,
    line_height: Pixels,
    window: &Window,
) -> Vec<PaintedLine> {
    let body_size = window.text_style().font_size.to_pixels(window.rem_size());
    let run_ends = run_ends(runs);
    let mut start = 0;
    let mut top = px(0.);
    let mut lines = Vec::new();
    for line_text in text.split('\n') {
        let end = start + line_text.len();
        let font_size = sized_lines
            .iter()
            .find(|(at, _)| *at == start)
            .map_or(body_size, |&(_, size)| size);
        let Ok(shaped) = window.text_system().shape_text(
            SharedString::from(line_text.to_owned()),
            font_size,
            &runs_within(runs, &run_ends, &(start..end)),
            wrap_width,
            None,
        ) else {
            return Vec::new();
        };
        for line in shaped {
            let height = line.size(line_height).height;
            lines.push(PaintedLine { start, top, line });
            top += height;
        }
        start = end + 1;
    }
    lines
}

/// The end offset of each of `runs` (which cover the text from offset 0).
pub(super) fn run_ends(runs: &[TextRun]) -> Vec<usize> {
    runs.iter()
        .scan(0, |end, run| {
            *end += run.len;
            Some(*end)
        })
        .collect()
}

/// The parts of `runs` (which cover the text from offset 0, ending at `ends`) inside byte
/// `range`.
fn runs_within(runs: &[TextRun], ends: &[usize], range: &Range<usize>) -> Vec<TextRun> {
    let first = ends.partition_point(|&end| end <= range.start);
    runs.iter()
        .zip(ends)
        .skip(first)
        .map(|(run, &end)| (run, end - run.len, end))
        .take_while(|&(_, start, _)| start < range.end)
        .filter_map(|(run, start, end)| {
            let len = end.min(range.end).saturating_sub(start.max(range.start));
            (len > 0).then(|| TextRun { len, ..run.clone() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use gpui::{TextRun, font, hsla};

    use super::{run_ends, runs_within};

    fn run(len: usize) -> TextRun {
        TextRun {
            len,
            font: font("IBM Plex Mono"),
            color: hsla(0., 0., 1., 1.),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[test]
    fn the_runs_within_a_line_are_cut_at_its_edges() {
        let runs = [run(3), run(4), run(0), run(5)];
        let ends = run_ends(&runs);
        assert_eq!(ends, [3, 7, 7, 12]);

        let lens = |range| -> Vec<usize> {
            runs_within(&runs, &ends, &range)
                .iter()
                .map(|run| run.len)
                .collect()
        };
        assert_eq!(lens(2..8), [1, 4, 1]);
        assert_eq!(lens(7..12), [5]);
        assert_eq!(lens(5..5), Vec::<usize>::new());
        assert_eq!(lens(0..3), [3]);
    }
}
