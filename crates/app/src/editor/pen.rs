//! Slot underlines drawn like an editor's pen: a little wavy, one stroke per row, with a
//! wobble that depends only on the distance from the start of the Slot.

use std::f32::consts::TAU;
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
use std::ops::Range;

use gpui::{Hsla, Path, PathBuilder, Pixels, Point, point, px};
use ka_teb_core::{Level, Slot, SlotId};

use super::Editor;
use super::painted_text::PaintedText;
use crate::Theme;

/// Word and Sentence underline: an editor's pen, a little wavy. Height of the
/// wobble, length of its main wave, base thickness and how much the thickness changes.
const PEN_WOBBLE: f32 = 0.8;
const PEN_WAVELENGTH: f32 = 60.;
const PEN_THICKNESS: f32 = 1.;
const PEN_THICKNESS_WOBBLE: f32 = 0.25;
/// Distance between the points of the pen line.
const PEN_STEP: Pixels = px(2.);

/// One row of a Slot underline in one opacity, drawn like an editor's pen.
#[derive(Debug, Clone)]
pub(super) struct PenStroke {
    pub(super) slot: SlotId,
    /// Window coordinates.
    pub(super) centre: Vec<Point<Pixels>>,
    pub(super) thickness: Vec<Pixels>,
    pub(super) color: Hsla,
}

impl PenStroke {
    /// The stroke outline as a filled path; `None` when it does not tessellate.
    pub(super) fn path(&self) -> Option<Path<Pixels>> {
        let half = |thickness: &Pixels| point(px(0.), *thickness / 2.);
        let top = self
            .centre
            .iter()
            .zip(&self.thickness)
            .map(|(at, thickness)| *at - half(thickness));
        let bottom = self
            .centre
            .iter()
            .zip(&self.thickness)
            .rev()
            .map(|(at, thickness)| *at + half(thickness));
        let outline: Vec<Point<Pixels>> = top.chain(bottom).collect();
        let mut builder = PathBuilder::fill();
        builder.add_polygon(&outline, true);
        builder.build().ok()
    }
}

/// A row of text under a pen stroke (local coordinates).
struct PenRow {
    top: Pixels,
    left: Pixels,
    right: Pixels,
}

/// The pen strokes under the Word and Sentence Slots: one per row and opacity. Each span
/// belongs to the innermost Slot over it, which gives its stroke its seed.
pub(super) fn pen_strokes(
    painted: &PaintedText,
    editor: &Editor,
    slots: &[Slot],
    theme: &Theme,
) -> Vec<PenStroke> {
    let text = editor.shown_document().text();
    editor
        .underline_spans()
        .into_iter()
        .filter(|span| span.underlined)
        .filter_map(|span| {
            let owner = slots
                .iter()
                .filter(|slot| {
                    slot.level != Level::Paragraph
                        && slot.range.start <= span.range.start
                        && span.range.end <= slot.range.end
                })
                .min_by_key(|slot| slot.range.len())?;
            let color = theme.slot_underline.opacity(span.opacity(theme));
            Some(slot_strokes(painted, text, &span.range, owner, color))
        })
        .flatten()
        .collect()
}

/// The pen strokes under `range` of `text` (window coordinates), one per row. The wobble
/// depends only on the distance from the start of `slot`, so it stays the same on every
/// paint and when text before the Slot changes.
fn slot_strokes(
    painted: &PaintedText,
    text: &str,
    range: &Range<usize>,
    slot: &Slot,
    color: Hsla,
) -> Vec<PenStroke> {
    let Some(slot_start) = painted.position_for(slot.range.start) else {
        return Vec::new();
    };
    let seed = BuildHasherDefault::<DefaultHasher>::default().hash_one(slot.id);
    let width = f32::from(painted.bounds.size.width);
    pen_rows(painted, text, range)
        .into_iter()
        .filter(|row| row.left < row.right)
        .map(|row| {
            let along = f32::from(row.left - slot_start.x)
                + (row.top - slot_start.y) / painted.line_height * width;
            let mut stroke = PenStroke {
                slot: slot.id,
                centre: Vec::new(),
                thickness: Vec::new(),
                color,
            };
            let baseline = painted.underline_y(row.top);
            let mut x = row.left;
            loop {
                let (offset, thickness) = pen(seed, along + f32::from(x - row.left));
                stroke
                    .centre
                    .push(painted.bounds.origin + point(x, baseline + px(offset)));
                stroke.thickness.push(px(thickness));
                if x >= row.right {
                    return stroke;
                }
                x = (x + PEN_STEP).min(row.right);
            }
        })
        .collect()
}

/// The rows `range` of `text` covers, each from its first to its last char edge.
fn pen_rows(painted: &PaintedText, text: &str, range: &Range<usize>) -> Vec<PenRow> {
    let offsets = text
        .get(range.clone())
        .unwrap_or_default()
        .char_indices()
        .map(|(index, _)| range.start + index)
        .chain([range.end]);
    let mut rows: Vec<PenRow> = Vec::new();
    for at in offsets.filter_map(|offset| painted.position_for(offset)) {
        if let Some(row) = rows.last_mut().filter(|row| row.top == at.y) {
            row.right = at.x;
            continue;
        }
        let left = if rows.is_empty() { at.x } else { px(0.) };
        rows.push(PenRow {
            top: at.y,
            left,
            right: at.x,
        });
    }
    rows
}

/// Offset from a straight line and thickness of a pen stroke `along` px into its Slot. Two
/// slow sines with phases from `seed`: a hand-drawn line, not a zigzag.
fn pen(seed: u64, along: f32) -> (f32, f32) {
    let phase = |shift: u32| f32::from(u16::try_from((seed >> shift) & 0x3ff).unwrap_or(0)) / 1024.;
    let wave = |wavelength: f32, shift: u32| ((along / wavelength + phase(shift)) * TAU).sin();
    let offset =
        PEN_WOBBLE * (0.65 * wave(PEN_WAVELENGTH, 0) + 0.35 * wave(PEN_WAVELENGTH * 0.37, 10));
    let thickness = PEN_THICKNESS + PEN_THICKNESS_WOBBLE * wave(PEN_WAVELENGTH * 0.61, 20);
    (offset, thickness)
}

#[cfg(test)]
mod tests {
    use super::pen;

    fn samples(seed: u64) -> Vec<(f32, f32)> {
        (0..400u16)
            .map(|along| pen(seed, f32::from(along)))
            .collect()
    }

    #[test]
    fn the_pen_line_wobbles_about_half_to_one_pixel() {
        let wobble = samples(7)
            .iter()
            .map(|(offset, _)| offset.abs())
            .fold(0., f32::max);
        assert!((0.5..=1.).contains(&wobble), "wobble {wobble}");
    }

    #[test]
    fn the_pen_line_thickness_changes_a_little() {
        let thickness: Vec<f32> = samples(7).iter().map(|(_, thickness)| *thickness).collect();
        let thinnest = thickness.iter().copied().fold(f32::MAX, f32::min);
        let thickest = thickness.iter().copied().fold(0., f32::max);
        assert!(thinnest >= 0.7 && thickest <= 1.3, "{thinnest}..{thickest}");
        assert!(thickest - thinnest > 0.2, "{thinnest}..{thickest}");
    }

    #[test]
    fn slots_with_other_seeds_draw_other_lines() {
        assert_ne!(samples(7), samples(0x5_0505_0505));
    }
}
