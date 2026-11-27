//! Focus zoom. The paragraph with the caret paints a little larger about the middle
//! of its rows; the layout stays, so the paragraph may overlap its neighbors a little.
//! GPUI has no transform for text or quads (`TransformationMatrix` is for SVG only),
//! so this paints the glyphs of the zoomed paragraph itself and moves every quad, the caret,
//! and the pen strokes over it. Mouse positions map back before the hit tests.

use std::ops::Range;

use gpui::{
    App, Bounds, FontId, GlyphId, Hsla, PaintQuad, Pixels, Point, TextRun, Window, fill, point, px,
};
use hamish_core::{Toggle, paragraph_index, paragraph_range};

use super::Editor;
use super::element::{EditorElement, EditorPrepaint};
use super::painted_text::{PaintedLine, PaintedText, run_at};
use super::pen::PenStroke;
use super::synthetic_bold;
use crate::settings;

/// One paragraph painted at `scale` about `centre`. `top` and `bottom` bound its rows
/// before the zoom (window coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Zoom {
    centre: Point<Pixels>,
    scale: f32,
    top: Pixels,
    bottom: Pixels,
}

impl Zoom {
    fn point(&self, at: Point<Pixels>) -> Point<Pixels> {
        self.centre + (at - self.centre) * self.scale
    }

    fn unpoint(&self, at: Point<Pixels>) -> Point<Pixels> {
        self.centre + (at - self.centre) / self.scale
    }

    fn bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        Bounds {
            origin: self.point(bounds.origin),
            size: gpui::size(
                bounds.size.width * self.scale,
                bounds.size.height * self.scale,
            ),
        }
    }

    /// `y` (before the zoom) is on a row of this paragraph.
    fn holds(&self, y: Pixels) -> bool {
        self.top <= y && y < self.bottom
    }
}

/// `bounds` as painted: zoomed when its middle is on a zoomed paragraph.
fn zoomed_bounds(zooms: &[Zoom], bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    zooms
        .iter()
        .find(|zoom| zoom.holds(bounds.center().y))
        .map_or(bounds, |zoom| zoom.bounds(bounds))
}

fn zoomed_quad(zooms: &[Zoom], quad: &PaintQuad) -> PaintQuad {
    PaintQuad {
        bounds: zoomed_bounds(zooms, quad.bounds),
        ..quad.clone()
    }
}

/// Where a pointer at `position` points in the text before the zoom.
fn unzoomed(zooms: &[Zoom], position: Point<Pixels>) -> Point<Pixels> {
    zooms
        .iter()
        .map(|zoom| (zoom, zoom.unpoint(position)))
        .find(|(zoom, at)| zoom.holds(at.y))
        .map_or(position, |(_, at)| at)
}

/// The zoomed paragraphs of one frame and their lines.
pub(super) struct Frame {
    pub(super) zooms: Vec<Zoom>,
    pub(super) lines: Vec<ZoomedLine>,
}

/// One glyph of a zoomed paragraph: its baseline origin before the zoom.
#[derive(Debug, Clone)]
struct Glyph {
    origin: Point<Pixels>,
    font_id: FontId,
    id: GlyphId,
    emoji: bool,
    color: Hsla,
    /// Paints a second time for synthetic bold.
    bold: bool,
}

/// A line of the text that paints zoomed in place of its normal paint.
#[derive(Clone)]
pub(super) struct ZoomedLine {
    /// Index in the painted lines.
    pub(super) line: usize,
    zoom: Zoom,
    font_size: Pixels,
    glyphs: Vec<Glyph>,
    backgrounds: Vec<PaintQuad>,
}

impl ZoomedLine {
    /// The glyphs of `painted` (its top left at `origin`) with the colors of `runs`, which
    /// cover the whole text from offset 0. A glyph in `bold` paints synthetic bold.
    fn new(
        line: usize,
        painted: &PaintedLine,
        origin: Point<Pixels>,
        line_height: Pixels,
        (runs, bold): (&[TextRun], &[Range<usize>]),
        zoom: Zoom,
    ) -> Self {
        let mut glyphs = Vec::new();
        let mut backgrounds = Vec::new();
        for glyph in painted.glyphs(origin, line_height) {
            let run = run_at(runs, glyph.offset);
            glyphs.push(Glyph {
                origin: glyph.baseline,
                font_id: glyph.font_id,
                id: glyph.id,
                emoji: glyph.emoji,
                color: run.map_or_else(Hsla::default, |run| run.color),
                bold: !glyph.emoji && bold.iter().any(|range| range.contains(&glyph.offset)),
            });
            let Some(background) = run.and_then(|run| run.background_color) else {
                continue;
            };
            backgrounds.push(zoom_quad_of(
                &zoom,
                glyph.top_left,
                glyph.width,
                line_height,
                background,
            ));
        }
        Self {
            line,
            zoom,
            font_size: painted.line.unwrapped_layout.font_size * zoom.scale,
            glyphs,
            backgrounds,
        }
    }

    /// A failed glyph paint leaves that glyph blank; the next frame tries again.
    pub(super) fn paint(&self, window: &mut Window) {
        for quad in &self.backgrounds {
            window.paint_quad(quad.clone());
        }
        for glyph in &self.glyphs {
            let origin = self.zoom.point(glyph.origin);
            let painted = if glyph.emoji {
                window.paint_emoji(origin, glyph.font_id, glyph.id, self.font_size)
            } else {
                window.paint_glyph(origin, glyph.font_id, glyph.id, self.font_size, glyph.color)
            };
            painted.ok();
            if glyph.bold {
                let origin = origin + synthetic_bold::spread(self.font_size);
                window
                    .paint_glyph(origin, glyph.font_id, glyph.id, self.font_size, glyph.color)
                    .ok();
            }
        }
    }
}

fn zoom_quad_of(
    zoom: &Zoom,
    top: Point<Pixels>,
    width: Pixels,
    height: Pixels,
    color: Hsla,
) -> PaintQuad {
    fill(
        zoom.bounds(Bounds::new(top, gpui::size(width, height))),
        color,
    )
}

impl Editor {
    /// Each paragraph shown zoomed now and its scale; the one with the caret first.
    /// Empty with the setting off and while the Alternatives panel dims the text.
    #[must_use]
    pub fn focus_zoom(&self, cx: &App) -> Vec<(Range<usize>, f32)> {
        let now = cx.background_executor().now();
        let text = self.session.document().text();
        self.focus_zoom
            .zoomed(now, settings::is_on(cx, Toggle::ReducedMotion))
            .into_iter()
            .filter_map(|(index, scale)| Some((paragraph_range(text, index)?, scale)))
            .collect()
    }

    /// Zooms the paragraph with the caret; `true` while a zoom runs.
    fn focus_zoom_step(&mut self, cx: &App) -> bool {
        let on = settings::is_on(cx, Toggle::FocusZoom) && self.bright_paragraph().is_none();
        let paragraph =
            on.then(|| paragraph_index(self.session.document().text(), self.caret_offset()));
        let now = cx.background_executor().now();
        let reduced_motion = settings::is_on(cx, Toggle::ReducedMotion);
        self.focus_zoom.focus(paragraph, now, reduced_motion);
        self.focus_zoom.is_moving(now, reduced_motion)
    }

    /// A pointer position as a position in the text before the zoom.
    pub(super) fn unzoomed(&self, position: Point<Pixels>) -> Point<Pixels> {
        unzoomed(&self.zooms, position)
    }

    /// `bounds` in the text as painted.
    pub(super) fn zoomed(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        zoomed_bounds(&self.zooms, bounds)
    }
}

impl EditorElement {
    /// The zoom of each zoomed paragraph and its glyphs; while a zoom runs, the next frame
    /// comes.
    pub(super) fn focus_zoom(
        &self,
        painted: &PaintedText,
        runs: (&[TextRun], &[Range<usize>]),
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        if self
            .editor
            .update(cx, |editor, cx| editor.focus_zoom_step(cx))
        {
            window.request_animation_frame();
        }
        let zoomed = self.editor.read(cx).focus_zoom(cx);
        let bounds = painted.bounds;
        let (zooms, lines) = zoomed
            .iter()
            .filter_map(|(range, scale)| {
                let index = painted
                    .lines
                    .iter()
                    .position(|line| line.start == range.start)?;
                let line = painted.lines.get(index)?;
                let origin = bounds.origin + point(px(0.), line.top);
                let height = line.line.size(painted.line_height).height;
                let zoom = Zoom {
                    centre: point(bounds.center().x, origin.y + height / 2.),
                    scale: *scale,
                    top: origin.y,
                    bottom: origin.y + height,
                };
                let glyphs = ZoomedLine::new(index, line, origin, painted.line_height, runs, zoom);
                Some((zoom, glyphs))
            })
            .unzip();
        Frame { zooms, lines }
    }
}

impl EditorPrepaint {
    /// Moves every quad, the caret, the pen strokes and the effects over a zoomed paragraph.
    pub(super) fn zoomed(mut self) -> Self {
        let zooms = self.zoom.zooms.clone();
        for quads in [
            &mut self.hover,
            &mut self.lab_marks,
            &mut self.selection,
            &mut self.marker_quads,
        ] {
            for quad in quads.iter_mut() {
                *quad = zoomed_quad(&zooms, quad);
            }
        }
        self.caret = self.caret.take().map(|quad| zoomed_quad(&zooms, &quad));
        for stroke in &mut self.underlines {
            *stroke = zoomed_stroke(&zooms, stroke);
        }
        for (_, _, rows) in &mut self.effects {
            for row in rows.iter_mut() {
                *row = zoomed_bounds(&zooms, *row);
            }
        }
        self
    }
}

fn zoomed_stroke(zooms: &[Zoom], stroke: &PenStroke) -> PenStroke {
    let zoom = stroke
        .centre
        .first()
        .and_then(|first| zooms.iter().find(|zoom| zoom.holds(first.y)));
    let Some(zoom) = zoom else {
        return stroke.clone();
    };
    PenStroke {
        slot: stroke.slot,
        centre: stroke.centre.iter().map(|at| zoom.point(*at)).collect(),
        thickness: stroke
            .thickness
            .iter()
            .map(|thickness| *thickness * zoom.scale)
            .collect(),
        color: stroke.color,
    }
}
