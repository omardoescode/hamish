//! FR-78, FR-79: the explosion of a Variant delete and the burst of its undo. Effects only
//! paint: they never block input (NFR "Fun, not in the way").

use std::ops::Range;
use std::time::Instant;

use gpui::{App, Bounds, BoxShadow, Hsla, Pixels, Point, Window, fill, point, px, rgb, size};
use ka_teb_core::Effect as Timing;

use crate::settings_modal;

const SPARKS: u64 = 36;
const FLAMES: u64 = 7;

/// Spec colors (estimate): core, flame, sparks.
fn core_color() -> Hsla {
    rgb(0x00ff_f2c0).into()
}

fn flame_color() -> Hsla {
    rgb(0x00ff_9a2e).into()
}

fn spark_color() -> Hsla {
    rgb(0x00e8_762a).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// FR-78: a Variant delete.
    Explosion,
    /// FR-79: the undo of a Variant delete.
    Burst,
}

/// An effect at a byte range of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub kind: EffectKind,
    pub range: Range<usize>,
}

/// An effect that plays: when it started, and the seed of its particles.
#[derive(Debug, Clone)]
pub(crate) struct Playing {
    pub(crate) effect: Effect,
    started: Instant,
    seed: u64,
}

impl Playing {
    pub(crate) fn new(effect: Effect, seed: u64) -> Self {
        Self {
            effect,
            started: Instant::now(),
            seed,
        }
    }

    /// 0 at the start, up to 1 at the end; `None` when it is over. With Reduced motion an
    /// effect does not play (see [`plays`]).
    pub(crate) fn progress(&self, now: Instant) -> Option<f32> {
        let timing = match self.effect.kind {
            EffectKind::Explosion => Timing::Explosion,
            EffectKind::Burst => Timing::UndoBurst,
        };
        let progress = timing.progress(now.saturating_duration_since(self.started), false);
        (progress < 1.).then_some(progress)
    }

    /// Paints the effect over the span `rows` (window coordinates, one per text row) at
    /// `progress`. `background` covers the text under the burst; it clears as the effect
    /// ends, so the new text shows.
    pub(crate) fn paint(
        &self,
        rows: &[Bounds<Pixels>],
        line_height: Pixels,
        progress: f32,
        background: Hsla,
        window: &mut Window,
    ) {
        let Some(area) = rows.iter().copied().reduce(|all, row| all.union(&row)) else {
            return;
        };
        let kind = self.effect.kind;
        let cover = match kind {
            EffectKind::Explosion => 1. - ramp(progress, 0.45, 1.),
            EffectKind::Burst => 1. - ramp(progress, 0.6, 1.),
        };
        for row in rows {
            window.paint_quad(fill(*row, background.opacity(cover)));
        }
        let glow = match kind {
            EffectKind::Explosion => (1. - progress).powi(2),
            EffectKind::Burst => (progress * std::f32::consts::PI).sin(),
        };
        let radius = line_height * (0.5 + 0.5 * ease_out(progress));
        for (color, blur) in [(flame_color(), 1.4), (core_color(), 0.7)] {
            let glow_bounds = Bounds::centered_at(area.center(), size(radius, radius * 0.6));
            window.paint_shadows(
                glow_bounds,
                (radius * 0.3).into(),
                &[BoxShadow {
                    color: color.opacity(glow * 0.9),
                    offset: point(px(0.), px(0.)),
                    blur_radius: radius * blur,
                    spread_radius: px(0.),
                }],
            );
        }
        for flame in flames(area, line_height, self.seed, progress, kind) {
            window.paint_quad(
                fill(flame, flame_color().opacity(glow * 0.7)).corner_radii(flame.size.height),
            );
        }
        let spark = if progress < 0.3 {
            core_color()
        } else {
            spark_color()
        };
        let fade = match kind {
            EffectKind::Explosion => 1. - progress,
            EffectKind::Burst => glow,
        };
        for bounds in sparks(area, line_height, self.seed, progress, kind) {
            window.paint_quad(fill(bounds, spark.opacity(fade)).corner_radii(px(1.)));
        }
    }
}

/// Whether effects play: with Reduced motion they show no particles and are done at once.
pub(crate) fn plays(cx: &App) -> bool {
    !settings_modal::reduced_motion(cx)
}

/// The spark squares at `progress`: they fly out from the span about one line above and
/// below and about one word to each side (spec); a Burst flies them in.
fn sparks(
    area: Bounds<Pixels>,
    line_height: Pixels,
    seed: u64,
    progress: f32,
    kind: EffectKind,
) -> Vec<Bounds<Pixels>> {
    let travel = match kind {
        EffectKind::Explosion => ease_out(progress),
        EffectKind::Burst => 1. - ease_out(progress),
    };
    (0..SPARKS)
        .map(|index| {
            let mut random = Random(seed ^ (index + 1).wrapping_mul(0x9e37_79b9));
            let start = point_in(area, random.unit(), random.unit());
            let angle = random.unit() * std::f32::consts::TAU;
            let reach = line_height * (0.4 + 0.6 * random.unit());
            let side = px(2.) + px(1.5) * random.unit();
            let center = Point {
                x: start.x + reach * (angle.cos() * travel),
                y: start.y + reach * (angle.sin() * travel),
            };
            Bounds::centered_at(center, size(side, side))
        })
        .collect()
}

/// The flame blobs along the span at `progress`.
fn flames(
    area: Bounds<Pixels>,
    line_height: Pixels,
    seed: u64,
    progress: f32,
    kind: EffectKind,
) -> Vec<Bounds<Pixels>> {
    let grow = match kind {
        EffectKind::Explosion => ease_out(progress),
        EffectKind::Burst => progress,
    };
    (0..FLAMES)
        .map(|index| {
            let mut random = Random(seed.rotate_left(17) ^ (index + 1).wrapping_mul(0x85eb_ca6b));
            let center = point_in(area, random.unit(), 0.3 + 0.4 * random.unit());
            let side = line_height * (0.25 + 0.35 * grow);
            Bounds::centered_at(center, size(side, side))
        })
        .collect()
}

fn point_in(area: Bounds<Pixels>, x: f32, y: f32) -> Point<Pixels> {
    point(
        area.origin.x + area.size.width * x,
        area.origin.y + area.size.height * y,
    )
}

fn ease_out(progress: f32) -> f32 {
    1. - (1. - progress).powi(3)
}

/// 0 before `from`, 1 after `to`, linear between.
fn ramp(progress: f32, from: f32, to: f32) -> f32 {
    ((progress - from) / (to - from)).clamp(0., 1.)
}

/// `SplitMix64`: particles differ per effect and stay the same in each frame of it.
struct Random(u64);

impl Random {
    /// A number in 0..1.
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^= mixed >> 31;
        let high = u16::try_from(mixed >> 48).unwrap_or(0);
        f32::from(high) / 65_536.
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui::{Bounds, point, px, size};

    use super::{Effect, EffectKind, Playing, sparks};

    fn playing(kind: EffectKind, age: Duration) -> Playing {
        Playing {
            effect: Effect { kind, range: 0..4 },
            started: Instant::now().checked_sub(age).unwrap(),
            seed: 7,
        }
    }

    #[test]
    fn an_explosion_ends_after_650_ms_and_a_burst_after_450_ms() {
        let now = Instant::now();
        let explosion = playing(EffectKind::Explosion, Duration::from_millis(600));
        assert!(explosion.progress(now).is_some());
        let explosion = playing(EffectKind::Explosion, Duration::from_millis(700));
        assert_eq!(explosion.progress(now), None);
        let burst = playing(EffectKind::Burst, Duration::from_millis(400));
        assert!(burst.progress(now).is_some());
        let burst = playing(EffectKind::Burst, Duration::from_millis(500));
        assert_eq!(burst.progress(now), None);
    }

    #[test]
    fn sparks_stay_within_about_one_line_around_the_span() {
        let line = px(27.);
        let span = Bounds::new(point(px(100.), px(200.)), size(px(80.), line));
        let reach = span.dilate(px(30.));
        for kind in [EffectKind::Explosion, EffectKind::Burst] {
            for step in 0..10_u8 {
                let progress = f32::from(step) / 10.;
                for spark in sparks(span, line, 42, progress, kind) {
                    assert!(reach.contains(&spark.center()), "{kind:?} at {progress}");
                }
            }
        }
    }

    #[test]
    fn exploding_sparks_fly_out_and_burst_sparks_fly_in() {
        let line = px(27.);
        let span = Bounds::new(point(px(100.), px(200.)), size(px(80.), line));
        let outside = |progress, kind| {
            sparks(span, line, 42, progress, kind)
                .iter()
                .filter(|spark| !span.contains(&spark.center()))
                .count()
        };
        assert!(outside(0.9, EffectKind::Explosion) > outside(0.0, EffectKind::Explosion));
        assert!(outside(0.9, EffectKind::Burst) < outside(0.0, EffectKind::Burst));
    }
}
