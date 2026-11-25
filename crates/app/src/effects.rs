//! Paints the Explosion of a Variant delete and the Undo burst of its undo. The particle
//! math is in core. Effects only paint, so they never block input.

use std::ops::Range;
use std::time::Instant;

use gpui::{App, Bounds, BoxShadow, Hsla, Pixels, Window, fill, point, px, rgb, size};
use ka_teb_core::{Effect, Particle, ParticleArea};

use crate::settings_modal;

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

/// A particle Effect at a byte range of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayingEffect {
    pub effect: Effect,
    pub range: Range<usize>,
}

/// A particle Effect that plays: when it started, and the seed of its particles.
#[derive(Debug, Clone)]
pub(crate) struct Particles {
    pub(crate) playing: PlayingEffect,
    started: Instant,
    seed: u64,
}

impl Particles {
    pub(crate) fn new(playing: PlayingEffect, seed: u64) -> Self {
        Self {
            playing,
            started: Instant::now(),
            seed,
        }
    }

    /// 0.0 at the start, up to 1.0 at the end; `None` when it is over. With Reduced motion
    /// an Effect does not play (see [`plays`]).
    pub(crate) fn progress(&self, now: Instant) -> Option<f32> {
        let progress = self
            .playing
            .effect
            .progress(now.saturating_duration_since(self.started), false);
        (progress < 1.).then_some(progress)
    }

    /// Paints the Effect over the span `rows` (window coordinates, one per text row) at
    /// `progress`. `background` covers the text under the burst; it clears as the Effect
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
        let effect = self.playing.effect;
        let Some(look) = effect.particle_look(progress) else {
            return;
        };
        for row in rows {
            window.paint_quad(fill(*row, background.opacity(look.cover)));
        }
        let radius = line_height * look.glow_size;
        for (color, blur) in [(flame_color(), 1.4), (core_color(), 0.7)] {
            let glow_bounds = Bounds::centered_at(area.center(), size(radius, radius * 0.6));
            window.paint_shadows(
                glow_bounds,
                (radius * 0.3).into(),
                &[BoxShadow {
                    color: color.opacity(look.glow * 0.9),
                    offset: point(px(0.), px(0.)),
                    blur_radius: radius * blur,
                    spread_radius: px(0.),
                }],
            );
        }
        let particle_area = ParticleArea {
            width: f32::from(area.size.width),
            height: f32::from(area.size.height),
            line_height: f32::from(line_height),
        };
        for flame in effect.flames(particle_area, self.seed, progress) {
            let bounds = bounds_in(area, flame);
            window.paint_quad(
                fill(bounds, flame_color().opacity(look.glow * 0.7))
                    .corner_radii(bounds.size.height),
            );
        }
        let spark = if progress < 0.3 {
            core_color()
        } else {
            spark_color()
        };
        for particle in effect.sparks(particle_area, self.seed, progress) {
            window.paint_quad(
                fill(bounds_in(area, particle), spark.opacity(look.spark)).corner_radii(px(1.)),
            );
        }
    }
}

fn bounds_in(area: Bounds<Pixels>, particle: Particle) -> Bounds<Pixels> {
    let center = point(
        area.origin.x + px(particle.x),
        area.origin.y + px(particle.y),
    );
    Bounds::centered_at(center, size(px(particle.side), px(particle.side)))
}

/// Whether Effects play: with Reduced motion they show no particles and are done at once.
pub(crate) fn plays(cx: &App) -> bool {
    !settings_modal::is_on(cx, |settings| settings.reduced_motion)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ka_teb_core::Effect;

    use super::{Particles, PlayingEffect};

    fn particles(effect: Effect, age: Duration) -> Particles {
        Particles {
            playing: PlayingEffect {
                effect,
                range: 0..4,
            },
            started: Instant::now().checked_sub(age).unwrap(),
            seed: 7,
        }
    }

    #[test]
    fn an_explosion_ends_after_650_ms_and_an_undo_burst_after_450_ms() {
        let now = Instant::now();
        let explosion = particles(Effect::Explosion, Duration::from_millis(600));
        assert!(explosion.progress(now).is_some());
        let explosion = particles(Effect::Explosion, Duration::from_millis(700));
        assert_eq!(explosion.progress(now), None);
        let burst = particles(Effect::UndoBurst, Duration::from_millis(400));
        assert!(burst.progress(now).is_some());
        let burst = particles(Effect::UndoBurst, Duration::from_millis(500));
        assert_eq!(burst.progress(now), None);
    }
}
