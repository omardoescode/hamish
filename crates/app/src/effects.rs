//! Paints the Explosion of a Variant delete and the Undo burst of its undo. The particle
//! math is in core. Effects only paint, so they never block input.

use std::ops::Range;
use std::time::Instant;

use gpui::{App, Bounds, BoxShadow, Hsla, Pixels, Window, fill, point, px, rgb, size};
use hamish_core::{Effect, Particle, ParticleArea, Toggle};

use crate::settings;

/// Explosion colors: core, flame, sparks.
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

/// The particle Effects of one view: each play gets its own particles; ended Effects drop.
#[derive(Debug, Default)]
pub(crate) struct EffectPlayer {
    playing: Vec<Particles>,
    seed: u64,
}

impl EffectPlayer {
    /// Starts `effect` over byte range `range`. With Reduced motion nothing plays.
    pub(crate) fn play(&mut self, effect: Effect, range: Range<usize>, cx: &App) {
        if !plays(cx) {
            return;
        }
        let now = cx.background_executor().now();
        self.playing
            .retain(|particles| particles.progress(now).is_some());
        self.seed = self.seed.wrapping_add(1);
        self.playing.push(Particles {
            playing: PlayingEffect { effect, range },
            started: now,
            seed: self.seed,
        });
    }

    /// The Effects that play at `now`, oldest first, each with its progress.
    pub(crate) fn frames(&self, now: Instant) -> Vec<(Particles, f32)> {
        self.playing
            .iter()
            .filter_map(|particles| Some((particles.clone(), particles.progress(now)?)))
            .collect()
    }

    /// The Effects that play now on the clock of the executor, oldest first.
    pub(crate) fn playing(&self, cx: &App) -> Vec<PlayingEffect> {
        self.frames(cx.background_executor().now())
            .into_iter()
            .map(|(particles, _)| particles.playing)
            .collect()
    }
}

/// A particle Effect that plays: when it started, and the seed of its particles.
#[derive(Debug, Clone)]
pub(crate) struct Particles {
    pub(crate) playing: PlayingEffect,
    started: Instant,
    seed: u64,
}

impl Particles {
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
            window.paint_drop_shadows(
                glow_bounds,
                (radius * 0.3).into(),
                &[BoxShadow {
                    color: color.opacity(look.glow * 0.9),
                    offset: point(px(0.), px(0.)),
                    blur_radius: radius * blur,
                    spread_radius: px(0.),
                    inset: false,
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
    !settings::is_on(cx, Toggle::ReducedMotion)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::TestAppContext;
    use hamish_core::{Effect, LoadedSettings, Settings};

    use super::{EffectPlayer, PlayingEffect};

    fn playing(effect: Effect) -> PlayingEffect {
        PlayingEffect {
            effect,
            range: 0..4,
        }
    }

    fn progress_at(
        player: &EffectPlayer,
        cx: &TestAppContext,
        after: Duration,
    ) -> Vec<(Effect, bool)> {
        let now = cx.executor().now() + after;
        player
            .frames(now)
            .into_iter()
            .map(|(particles, progress)| (particles.playing.effect, progress < 1.))
            .collect()
    }

    #[test]
    fn an_explosion_plays_650_ms_and_an_undo_burst_450_ms() {
        crate::system_text::run(1, |cx| {
            let mut player = EffectPlayer::default();
            cx.update(|cx| {
                player.play(Effect::Explosion, 0..4, cx);
                player.play(Effect::UndoBurst, 0..4, cx);
            });

            assert_eq!(
                cx.update(|cx| player.playing(cx)),
                vec![playing(Effect::Explosion), playing(Effect::UndoBurst)]
            );
            assert_eq!(
                progress_at(&player, cx, Duration::from_millis(400)),
                vec![(Effect::Explosion, true), (Effect::UndoBurst, true)]
            );
            assert_eq!(
                progress_at(&player, cx, Duration::from_millis(600)),
                vec![(Effect::Explosion, true)]
            );
            assert_eq!(progress_at(&player, cx, Duration::from_millis(700)), vec![]);
        });
    }

    #[test]
    fn two_effects_get_different_particles() {
        crate::system_text::run(1, |cx| {
            let mut player = EffectPlayer::default();
            cx.update(|cx| {
                player.play(Effect::Explosion, 0..4, cx);
                player.play(Effect::Explosion, 0..4, cx);
            });

            let seeds: Vec<u64> = player
                .frames(cx.executor().now())
                .into_iter()
                .map(|(particles, _)| particles.seed)
                .collect();
            assert_eq!(seeds, vec![1, 2]);
        });
    }

    #[test]
    fn with_reduced_motion_nothing_plays() {
        crate::system_text::run(1, |cx| {
            let mut player = EffectPlayer::default();
            cx.update(|cx| {
                let settings = Settings {
                    reduced_motion: hamish_core::OnOff::On,
                    ..Settings::default()
                };
                crate::settings::use_settings(
                    None,
                    LoadedSettings {
                        settings,
                        problem: None,
                    },
                    cx,
                );
                player.play(Effect::Explosion, 0..4, cx);
            });

            assert_eq!(cx.update(|cx| player.playing(cx)), vec![]);
        });
    }
}
