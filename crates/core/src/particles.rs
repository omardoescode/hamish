//! The particles of an Explosion and an Undo burst: where each spark and flame is at a
//! progress, and how strong the glow is. Views paint them.

use crate::Effect;

const SPARKS: u64 = 36;
const FLAMES: u64 = 7;

/// The span a particle Effect plays over, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleArea {
    pub width: f32,
    pub height: f32,
    pub line_height: f32,
}

/// A square particle: its center in pixels from the top-left corner of the span, and its side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub side: f32,
}

/// The look of a particle Effect at one progress. Each value is in 0.0..=1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleLook {
    /// Opacity of the background over the span: it hides the old text, then clears.
    pub cover: f32,
    /// Strength of the glow and the flames.
    pub glow: f32,
    /// Opacity of the sparks.
    pub spark: f32,
    /// Size of the glow, in line heights.
    pub glow_size: f32,
}

impl Effect {
    /// The look at `progress` (0.0..=1.0). `None` for an Effect with no particles.
    #[must_use]
    pub fn particle_look(self, progress: f32) -> Option<ParticleLook> {
        let glow_size = 0.5 + 0.5 * ease_out(progress);
        match self {
            Self::Explosion => {
                let glow = (1. - progress).powi(2);
                Some(ParticleLook {
                    cover: 1. - ramp(progress, 0.45, 1.),
                    glow,
                    spark: 1. - progress,
                    glow_size,
                })
            }
            Self::UndoBurst => {
                let glow = (progress * std::f32::consts::PI).sin();
                Some(ParticleLook {
                    cover: 1. - ramp(progress, 0.6, 1.),
                    glow,
                    spark: glow,
                    glow_size,
                })
            }
            Self::VariantFadeIn | Self::FocusZoom | Self::QuickFillDance => None,
        }
    }

    /// The sparks at `progress`. An Explosion flies them out from the span, about one line
    /// above and below and one word to each side; an Undo burst flies them in. `seed`
    /// picks the pattern: the same seed gives the same sparks in each frame. Empty for an
    /// Effect with no particles.
    #[must_use]
    pub fn sparks(self, area: ParticleArea, seed: u64, progress: f32) -> Vec<Particle> {
        let travel = match self {
            Self::Explosion => ease_out(progress),
            Self::UndoBurst => 1. - ease_out(progress),
            Self::VariantFadeIn | Self::FocusZoom | Self::QuickFillDance => return Vec::new(),
        };
        (0..SPARKS)
            .map(|index| {
                let mut random = Random(seed ^ (index + 1).wrapping_mul(0x9e37_79b9));
                let x = area.width * random.unit();
                let y = area.height * random.unit();
                let angle = random.unit() * std::f32::consts::TAU;
                let reach = area.line_height * (0.4 + 0.6 * random.unit());
                let side = 2. + 1.5 * random.unit();
                Particle {
                    x: x + reach * angle.cos() * travel,
                    y: y + reach * angle.sin() * travel,
                    side,
                }
            })
            .collect()
    }

    /// The flame blobs along the span at `progress`; they grow as it runs. Empty for an
    /// Effect with no particles.
    #[must_use]
    pub fn flames(self, area: ParticleArea, seed: u64, progress: f32) -> Vec<Particle> {
        let grow = match self {
            Self::Explosion => ease_out(progress),
            Self::UndoBurst => progress,
            Self::VariantFadeIn | Self::FocusZoom | Self::QuickFillDance => return Vec::new(),
        };
        (0..FLAMES)
            .map(|index| {
                let mut random =
                    Random(seed.rotate_left(17) ^ (index + 1).wrapping_mul(0x85eb_ca6b));
                Particle {
                    x: area.width * random.unit(),
                    y: area.height * (0.3 + 0.4 * random.unit()),
                    side: area.line_height * (0.25 + 0.35 * grow),
                }
            })
            .collect()
    }
}

fn ease_out(progress: f32) -> f32 {
    1. - (1. - progress).powi(3)
}

/// 0 before `from`, 1 after `to`, linear between.
fn ramp(progress: f32, from: f32, to: f32) -> f32 {
    ((progress - from) / (to - from)).clamp(0., 1.)
}

/// `SplitMix64`: particles differ per Effect and stay the same in each frame of it.
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
    use super::ramp;

    #[test]
    fn a_ramp_is_flat_outside_its_range_and_linear_inside() {
        assert!(ramp(0.2, 0.45, 1.).abs() < 1e-6);
        assert!((ramp(0.725, 0.45, 1.) - 0.5).abs() < 1e-6);
        assert!((ramp(1., 0.45, 1.) - 1.).abs() < 1e-6);
    }
}
