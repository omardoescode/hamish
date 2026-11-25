//! The loading dances (FR-34): three dots that hop in turn while the AI works, and the
//! "??" of a quick fill (FR-71).

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, FontWeight, Hsla, IntoElement, bounce, div, ease_in_out, prelude::*,
    px,
};

/// How high a dot hops.
const HOP: f32 = 3.;
/// One round of the three hops.
const BEAT: Duration = Duration::from_millis(900);

/// One round of the "??" poses: about 3 pose changes per second (spec: 2 to 4).
const QUICK_FILL_BEAT: Duration = Duration::from_millis(1300);

/// The poses of each "??" glyph in one round: the glyph and how far it lifts.
const QUICK_FILL_POSES: [[(&str, f32); 4]; 2] = [
    [("?", 0.), ("?", -2.), ("\u{bf}", 1.), ("?", 1.)],
    [("?", 0.), ("!", 1.), ("?", -2.), ("?", -1.)],
];

/// The "??" of a quick fill in bold `color`: the glyphs wiggle and flip in turn.
pub(crate) fn quick_fill_dance(id: &'static str, color: Hsla) -> impl IntoElement {
    div()
        .flex()
        .text_color(color)
        .font_weight(FontWeight::BOLD)
        .children(
            QUICK_FILL_POSES
                .into_iter()
                .zip(0_usize..)
                .map(move |(poses, glyph)| {
                    div().relative().with_animation(
                        (id, glyph),
                        Animation::new(QUICK_FILL_BEAT).repeat(),
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
                }),
        )
}

/// Three dots of `color` that hop in turn. `id` names the dance among its siblings.
pub(crate) fn dance(id: &'static str, color: Hsla) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(3.))
        .h(px(12.))
        .children((0..3_u8).map(move |dot| {
            let phase = f32::from(dot) / 3.;
            div()
                .relative()
                .size(px(4.))
                .rounded_full()
                .bg(color)
                .with_animation(
                    (id, usize::from(dot)),
                    Animation::new(BEAT).repeat(),
                    move |dot, delta| {
                        let lift = bounce(ease_in_out)((delta + phase) % 1.);
                        dot.top(px(-HOP * lift))
                    },
                )
        }))
}
