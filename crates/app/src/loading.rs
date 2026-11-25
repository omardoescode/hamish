//! The loading dance (FR-34): three dots that hop in turn while the AI works.

use std::time::Duration;

use gpui::{Animation, AnimationExt, Hsla, IntoElement, bounce, div, ease_in_out, prelude::*, px};

/// How high a dot hops.
const HOP: f32 = 3.;
/// One round of the three hops.
const BEAT: Duration = Duration::from_millis(900);

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
