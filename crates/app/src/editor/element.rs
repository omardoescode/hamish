//! The Editor's GPUI element: layout, prepaint and paint of the text with its Markers,
//! hover box, selection, Lab marks, pen underlines, Effects and caret.

use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, PaintQuad, Pixels, Point, SharedString, Size, Style, TextRun,
    Window, fill, point, px, relative, size,
};
use hamish_core::{Level, Slot, SlotId, Toggle, VimMode};

use super::painted_text::{PaintedLine, PaintedText, shape_sized};
use super::pen::{PenStroke, pen_strokes};
use super::runs::text_runs;
use super::synthetic_bold::{self, BoldGlyph};
use super::{Editor, Marker, MarkerKind, Spotlight, focus_zoom};
use crate::caret;
use crate::effects::Particles;
use crate::{Theme, settings};

const DOT_SIZE: Pixels = px(4.);
const DOT_GAP: Pixels = px(3.);
/// Paragraph Marker: a line about 2 px wide, about 14 px left of the text.
const MARGIN_LINE_WIDTH: Pixels = px(2.);
const MARGIN_LINE_GAP: Pixels = px(14.);
/// How far left each nesting level moves a Paragraph margin line.
const MARGIN_LINE_STEP: Pixels = px(10.);
const HOVER_PAD: Pixels = px(2.);
const HOVER_RADIUS: Pixels = px(3.);

/// The Normal mode block shows the accent under the char, which stays readable on it.
const BLOCK_CARET_OPACITY: f32 = 0.4;

/// Width left of the text that a Paragraph Marker and its hover area take.
const PARAGRAPH_MARGIN: Pixels = px(32.);

/// One Variant dot of the Marker of a Slot (window coordinates).
pub(super) type MarkerDot = (SlotId, Bounds<Pixels>);

pub(super) struct EditorElement {
    pub(super) editor: Entity<Editor>,
}

pub(super) struct EditorPrepaint {
    pub(super) lines: Vec<PaintedLine>,
    /// Each playing effect, its progress, and the rows of its span.
    pub(super) effects: Vec<(Particles, f32, Vec<Bounds<Pixels>>)>,
    pub(super) hover: Vec<PaintQuad>,
    pub(super) lab_marks: Vec<PaintQuad>,
    pub(super) selection: Vec<PaintQuad>,
    pub(super) marker_quads: Vec<PaintQuad>,
    pub(super) underlines: Vec<PenStroke>,
    pub(super) caret: Option<PaintQuad>,
    /// The Normal mode block: it paints under the text, so the char on it shows.
    pub(super) caret_behind_text: bool,
    pub(super) zoom: focus_zoom::Frame,
    /// Synthetic bold glyphs of the lines that are not zoomed.
    pub(super) bold: Vec<BoldGlyph>,
}

impl EditorElement {
    /// The effects that play now, each with its progress and the rows of its span. While one
    /// plays or a Variant fades in, the next frame comes.
    fn effect_frames(
        &self,
        painted: &PaintedText,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<(Particles, f32, Vec<Bounds<Pixels>>)> {
        let now = cx.background_executor().now();
        let effects: Vec<_> = self
            .editor
            .read(cx)
            .effects
            .frames(now)
            .into_iter()
            .map(|(playing, progress)| {
                let rows = painted
                    .text_quads(&playing.playing.range, Hsla::default())
                    .into_iter()
                    .map(|quad| quad.bounds)
                    .collect();
                (playing, progress, rows)
            })
            .collect();
        let fading = self.editor.read(cx).fading_in(cx).is_some();
        if fading || !effects.is_empty() {
            window.request_animation_frame();
        }
        effects
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let text: SharedString = self
            .editor
            .read(cx)
            .shown_document()
            .text()
            .to_owned()
            .into();
        let (runs, sized_lines) = text_runs(self.editor.read(cx), window, cx);
        let line_height = window.line_height();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, _| {
                let wrap_width = known.width.or(match available.width {
                    gpui::AvailableSpace::Definite(width) => Some(width),
                    gpui::AvailableSpace::MinContent | gpui::AvailableSpace::MaxContent => None,
                });
                let lines =
                    shape_sized(&text, &runs, &sized_lines, wrap_width, line_height, window);
                let height = lines.last().map_or(line_height, |last| {
                    last.top + last.line.size(line_height).height
                });
                Size {
                    width: wrap_width.unwrap_or_default(),
                    height,
                }
            });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let text: SharedString = self
            .editor
            .read(cx)
            .shown_document()
            .text()
            .to_owned()
            .into();
        let (runs, sized_lines) = text_runs(self.editor.read(cx), window, cx);
        let line_height = window.line_height();
        let lines = shape_sized(
            &text,
            &runs,
            &sized_lines,
            Some(bounds.size.width),
            line_height,
            window,
        );
        let painted = PaintedText {
            bounds,
            line_height,
            lines,
        };
        let theme = cx.global::<Theme>().clone();
        let editor = self.editor.read(cx);
        let shown = editor.shown_document();
        let selection =
            painted.selection_quads(&editor.to_shown(&editor.selection()), theme.selection);
        let lab_marks = lab_mark_quads(&painted, editor, &theme);
        let slots = shown.slots();
        let shown_in_panel = match editor.spotlight {
            Some(Spotlight::Slot(id)) => Some(id),
            Some(Spotlight::Text(_)) | None => None,
        };
        let tinted = editor
            .hovered
            .into_iter()
            .chain(shown_in_panel.filter(|id| editor.hovered != Some(*id)));
        let hover: Vec<PaintQuad> = tinted
            .filter_map(|id| slots.iter().find(|slot| slot.id == id))
            .flat_map(|slot| {
                hover_quads(
                    &painted,
                    &slot.range,
                    slot.level,
                    theme.slot_hover_background,
                )
            })
            .collect();
        let marked = marked_slots(&slots, cx);
        let (markers, marker_quads, marker_dots) =
            slot_markers(&painted, shown.text(), marked, &theme);
        let underlines = pen_strokes(&painted, editor, marked, &theme);
        let caret_behind_text =
            editor.drop_target.is_none() && editor.vim_mode(cx) == Some(VimMode::Normal);
        let caret = if caret_behind_text {
            let end_width = line_end_caret_width(&runs, window);
            let color = theme.accent.opacity(BLOCK_CARET_OPACITY);
            caret_at(editor)
                .and_then(|at| caret::block_caret_quad(&painted, &text, at, end_width, color))
        } else {
            caret_at(editor).and_then(|at| caret::caret_quad(&painted, at, theme.accent))
        };
        if editor.autoscroll {
            let inset = crate::workspace::chrome_bar_height(cx);
            caret::scroll_to_caret(&editor.scroll_handle, caret.as_ref(), inset, cx);
            window.request_animation_frame();
        }
        let text_font = runs
            .first()
            .map_or_else(SharedString::default, |run| run.font.family.clone());
        let bold_ranges = synthetic_bold::ranges(&runs, window);
        let zoom = self.focus_zoom(&painted, (&runs, &bold_ranges), window, cx);
        let mut bold = synthetic_bold::glyphs(&painted, &runs, &bold_ranges);
        let synthetic_bold: Vec<usize> = bold.iter().map(|glyph| glyph.offset).collect();
        bold.retain(|glyph| !zoom.lines.iter().any(|zoomed| zoomed.line == glyph.line));
        self.editor.update(cx, |editor, _| {
            editor.text_font = text_font;
            editor.synthetic_bold = synthetic_bold;
            editor.autoscroll = false;
            editor.markers = markers;
            editor.marker_dots = marker_dots;
            editor.hover_tint = hover.iter().map(|quad| quad.bounds).collect();
            editor.underlines.clone_from(&underlines);
        });
        EditorPrepaint {
            effects: self.effect_frames(&painted, window, cx),
            zoom,
            bold,
            lines: painted.lines,
            hover,
            lab_marks,
            selection,
            marker_quads,
            underlines,
            caret,
            caret_behind_text,
        }
        .zoomed()
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        let dropping = self.editor.read(cx).drop_target.is_some();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for quad in prepaint
            .hover
            .drain(..)
            .chain(prepaint.lab_marks.drain(..))
            .chain(prepaint.selection.drain(..))
            .chain(prepaint.marker_quads.drain(..))
        {
            window.paint_quad(quad);
        }
        let caret = prepaint.caret.take().filter(|_| {
            let focused = focus_handle.is_focused(window);
            dropping || focused && self.editor.read(cx).blink.shown(window, cx)
        });
        let caret_bounds = caret.as_ref().map(|caret| caret.bounds);
        if prepaint.caret_behind_text
            && let Some(caret) = caret.clone()
        {
            window.paint_quad(caret);
        }
        let line_height = window.line_height();
        let visible = window.content_mask().bounds;
        let in_view =
            |top: Pixels, bottom: Pixels| top <= visible.bottom() && bottom >= visible.top();
        let normal = prepaint.lines.iter().enumerate().filter(|(index, _)| {
            !prepaint
                .zoom
                .lines
                .iter()
                .any(|zoomed| zoomed.line == *index)
        });
        for (_, painted) in normal {
            let origin = bounds.origin + point(px(0.), painted.top);
            if !in_view(origin.y, origin.y + painted.line.size(line_height).height) {
                continue;
            }
            // A failed glyph paint leaves that line blank; the next frame tries again.
            painted
                .line
                .paint(origin, line_height, gpui::TextAlign::Left, None, window, cx)
                .ok();
        }
        for glyph in &prepaint.bold {
            glyph.paint(window);
        }
        for zoomed in &prepaint.zoom.lines {
            zoomed.paint(window);
        }
        let background = cx.global::<Theme>().background;
        for (playing, progress, rows) in prepaint.effects.drain(..) {
            playing.paint(&rows, line_height, progress, background, window);
        }
        let strokes = prepaint.underlines.iter().filter(|stroke| {
            stroke
                .centre
                .iter()
                .any(|at| in_view(at.y - line_height, at.y + line_height))
        });
        for stroke in strokes {
            if let Some(path) = stroke.path() {
                window.paint_path(path, stroke.color);
            }
        }
        if !prepaint.caret_behind_text
            && let Some(caret) = caret
        {
            window.paint_quad(caret);
        }
        let lines = std::mem::take(&mut prepaint.lines);
        let zooms = std::mem::take(&mut prepaint.zoom.zooms);
        self.editor.update(cx, |editor, _| {
            editor.zooms = zooms;
            editor.caret_bounds = caret_bounds;
            editor.painted = Some(PaintedText {
                bounds,
                line_height,
                lines,
            });
        });
    }
}

/// `local` lies on the text of `range`. A Paragraph also covers its full rows and
/// its left margin, where its Marker is.
pub(super) fn covers(
    painted: &PaintedText,
    range: &Range<usize>,
    level: Level,
    local: Point<Pixels>,
) -> bool {
    let Some((start, end)) = painted.rows(range) else {
        return false;
    };
    if local.y < start.y || local.y >= end.y + painted.line_height {
        return false;
    }
    if level == Level::Paragraph {
        return local.x >= -PARAGRAPH_MARGIN && local.x <= painted.bounds.size.width;
    }
    let first_row = local.y < start.y + painted.line_height;
    let last_row = local.y >= end.y;
    let left = if first_row { start.x } else { px(0.) };
    let right = if last_row {
        end.x
    } else {
        painted.bounds.size.width
    };
    local.x >= left && local.x < right
}

/// The hover box behind `range` (window coordinates): one rounded box per row for a
/// Word or Sentence Slot, one box over the full rows for a Paragraph.
fn hover_quads(
    painted: &PaintedText,
    range: &Range<usize>,
    level: Level,
    color: Hsla,
) -> Vec<PaintQuad> {
    let Some((start, end)) = painted.rows(range) else {
        return Vec::new();
    };
    let origin = painted.bounds.origin;
    if level == Level::Paragraph {
        let bounds = Bounds::from_corners(
            origin + point(-HOVER_PAD * 2., start.y),
            origin
                + point(
                    painted.bounds.size.width + HOVER_PAD * 2.,
                    end.y + painted.line_height,
                ),
        );
        return vec![fill(bounds, color).corner_radii(HOVER_RADIUS)];
    }
    painted
        .text_quads(range, color)
        .into_iter()
        .map(|quad| {
            let bounds = Bounds::from_corners(
                quad.bounds.origin - point(HOVER_PAD, px(0.)),
                quad.bounds.bottom_right() + point(HOVER_PAD, px(0.)),
            );
            fill(bounds, color).corner_radii(HOVER_RADIUS)
        })
        .collect()
}

/// The Slots that show a Marker: all of them, or none while the Markers setting is off.
fn marked_slots<'a, 'b>(slots: &'a [Slot<'b>], cx: &App) -> &'a [Slot<'b>] {
    if settings::is_on(cx, Toggle::Markers) {
        slots
    } else {
        &[]
    }
}

/// The Marker of each Slot in `slots`, the quads that draw them and the bounds of each
/// Variant dot (window coordinates).
fn slot_markers(
    painted: &PaintedText,
    text: &str,
    slots: &[Slot],
    theme: &Theme,
) -> (Vec<Marker>, Vec<PaintQuad>, Vec<MarkerDot>) {
    let mut markers = Vec::new();
    let mut marker_quads = Vec::new();
    let mut marker_dots = Vec::new();
    for (index, slot) in slots.iter().enumerate() {
        let marker = Marker {
            slot: slot.id,
            kind: match slot.level {
                Level::Word => MarkerKind::Underline,
                Level::Sentence => MarkerKind::SentenceUnderline,
                Level::Paragraph => MarkerKind::MarginLine,
            },
            dots: slot.variants.len(),
            active: slot.active,
        };
        let depth = slots
            .iter()
            .take(index)
            .filter(|outer| {
                outer.level == Level::Paragraph
                    && outer.range.start <= slot.range.start
                    && slot.range.end <= outer.range.end
            })
            .count();
        let quads = marker_quads_for(painted, text, &slot.range, &marker, depth, theme);
        let dots = quads.iter().rev().take(marker.dots).rev();
        marker_dots.extend(dots.map(|quad| (slot.id, quad.bounds)));
        marker_quads.extend(quads);
        markers.push(marker);
    }
    (markers, marker_quads, marker_dots)
}

/// The Marker of a Slot over `range` (window coordinates), less the underline, which is
/// a text run. `depth`: how many Paragraph Slots hold it; each moves a margin line left.
fn marker_quads_for(
    painted: &PaintedText,
    text: &str,
    range: &Range<usize>,
    marker: &Marker,
    depth: usize,
    theme: &Theme,
) -> Vec<PaintQuad> {
    let Some((start, end)) = painted.rows(range) else {
        return Vec::new();
    };
    let origin = painted.bounds.origin;
    let dots_length = (DOT_SIZE + DOT_GAP) * as_factor(marker.dots) - DOT_GAP;
    let bottom = end.y + painted.line_height;
    let row_left = if start.y == end.y { start.x } else { px(0.) };
    let line_x = -MARGIN_LINE_GAP - MARGIN_LINE_STEP * as_factor(depth);
    let (first_dot, dot_step, line) = match marker.kind {
        MarkerKind::Underline => (
            point(
                (row_left + end.x - dots_length) / 2.,
                bottom - DOT_SIZE - px(1.),
            ),
            point(DOT_SIZE + DOT_GAP, px(0.)),
            None,
        ),
        MarkerKind::SentenceUnderline => (
            sentence_dots_at(painted, text, range.end, end, dots_length),
            point(DOT_SIZE + DOT_GAP, px(0.)),
            None,
        ),
        MarkerKind::MarginLine => (
            point(
                line_x - DOT_GAP - DOT_SIZE,
                (start.y + bottom - dots_length) / 2.,
            ),
            point(px(0.), DOT_SIZE + DOT_GAP),
            Some(fill(
                Bounds::from_corners(
                    origin + point(line_x, start.y),
                    origin + point(line_x + MARGIN_LINE_WIDTH, bottom),
                ),
                theme.slot_paragraph_line,
            )),
        ),
    };
    let dots = (0..marker.dots).map(|index| {
        let at = origin + first_dot + dot_step * as_factor(index);
        let quad = fill(
            Bounds::new(at, size(DOT_SIZE, DOT_SIZE)),
            theme.slot_dot_filled,
        )
        .corner_radii(DOT_SIZE / 2.);
        if index == marker.active {
            return quad;
        }
        quad.background(gpui::transparent_black())
            .border_widths(px(1.))
            .border_color(theme.slot_dot)
    });
    line.into_iter().chain(dots).collect()
}

/// Where the first Variant dot of a Sentence Marker goes (local coordinates): after the
/// sentence's last char, in the middle of its row. When the gap to the next text on the
/// row or to the right edge is too narrow, the dots go under the row and end at the last char.
fn sentence_dots_at(
    painted: &PaintedText,
    text: &str,
    end_offset: usize,
    end: Point<Pixels>,
    dots_length: Pixels,
) -> Point<Pixels> {
    let after = end.x + DOT_GAP * 2.;
    let next_text = text
        .get(end_offset..)
        .unwrap_or_default()
        .trim_start_matches(' ');
    let next_on_line = !next_text.is_empty() && !next_text.starts_with('\n');
    let room = painted
        .position_for(text.len() - next_text.len())
        .filter(|next| next_on_line && next.y == end.y)
        .map_or(painted.bounds.size.width, |next| next.x - DOT_GAP * 2.);
    if after + dots_length <= room {
        return point(after, end.y + (painted.line_height - DOT_SIZE) / 2.);
    }
    point(
        (end.x - dots_length).max(px(0.)),
        end.y + painted.line_height - DOT_SIZE - px(1.),
    )
}

/// The Lab marks and the Trim cut being walked through, as highlight quads. The selected
/// Lab mark gets an accent outline.
fn lab_mark_quads(painted: &PaintedText, editor: &Editor, theme: &Theme) -> Vec<PaintQuad> {
    let walked = editor.document().trim_proposal().and_then(|proposal| {
        proposal
            .current()
            .and_then(|current| proposal.cuts().get(current))
    });
    let selected = editor.selected_lab_mark().map(|mark| mark.id);
    let marks = editor
        .document()
        .lab_marks()
        .iter()
        .map(|mark| (&mark.range, Some(mark.id) == selected));
    marks
        .chain(walked.map(|cut| (&cut.range, false)))
        .flat_map(|(range, outlined)| {
            painted
                .text_quads(range, theme.lab_mark)
                .into_iter()
                .map(move |quad| {
                    if outlined {
                        quad.border_widths(px(1.))
                            .border_color(theme.accent_outline)
                    } else {
                        quad
                    }
                })
        })
        .collect()
}

/// `count` as a factor for pixel math; counts past `u16::MAX` stop there.
fn as_factor(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

/// Offset in the shown Document of the caret to paint: the drop target, else the caret;
/// `None` with a selection.
fn caret_at(editor: &Editor) -> Option<usize> {
    match editor.drop_target {
        Some(at) => Some(editor.to_shown(&(at..at)).start),
        None => editor.selection().is_empty().then(|| {
            editor
                .to_shown(&(editor.caret_offset()..editor.caret_offset()))
                .start
        }),
    }
}

/// Width of the Normal mode block on a line end: the advance of an average char of the
/// first text run; half the font size when the font gives none.
fn line_end_caret_width(runs: &[TextRun], window: &Window) -> Pixels {
    let font_size = window.text_style().font_size.to_pixels(window.rem_size());
    runs.first()
        .and_then(|run| {
            let text_system = window.text_system();
            text_system
                .ch_advance(text_system.resolve_font(&run.font), font_size)
                .ok()
        })
        .unwrap_or(font_size / 2.)
}
