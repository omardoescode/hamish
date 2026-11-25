//! The loading dance of the Alternatives panel: a "??" that wiggles in the empty row while
//! the AI works, for a quick fill and for "AI alternatives".

use gpui::{Animation, AnimationExt, AnyElement, FontWeight, Hsla, div, prelude::*, px};
use ka_teb_core::Effect;

/// The poses of each "??" glyph in one round: the glyph and how far it lifts.
const POSES: [[(&str, f32); 4]; 2] = [
    [("?", 0.), ("?", -2.), ("\u{bf}", 1.), ("?", 1.)],
    [("?", 0.), ("!", 1.), ("?", -2.), ("?", -1.)],
];

/// The "??" in bold `color`: the glyphs wiggle and flip in turn. With `reduced_motion` the
/// "??" stays still.
pub(crate) fn dance(id: &'static str, color: Hsla, reduced_motion: bool) -> AnyElement {
    let glyphs = div().flex().text_color(color).font_weight(FontWeight::BOLD);
    if reduced_motion {
        return glyphs
            .id(id)
            .debug_selector(|| "alternatives-loading-still".to_owned())
            .child("??")
            .into_any_element();
    }
    glyphs
        .children(POSES.into_iter().zip(0_usize..).map(move |(poses, glyph)| {
            div().relative().with_animation(
                (id, glyph),
                Animation::new(Effect::QuickFillDance.duration()).repeat(),
                move |element, delta| {
                    let at = (delta * 4.).floor();
                    let (text, lift) = poses
                        .iter()
                        .zip(0_u8..)
                        .find(|&(_, index)| f32::from(index) >= at)
                        .map_or(("?", 0.), |(pose, _)| *pose);
                    element.top(px(lift)).child(text)
                },
            )
        }))
        .into_any_element()
}
