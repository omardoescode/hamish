//! Cycle sounds (issue #27) and the Variant delete explosion (issue #30): which sound
//! plays, and its samples. The app sends the samples to the audio device.

use std::f64::consts::TAU;

use crate::Switch;

/// A sound the app plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// A switch to a Variant that is not the Original.
    HighTone,
    /// A switch back to the Original.
    LowTone,
    /// A Variant delete (issue #30).
    Explosion,
}

impl Switch {
    /// The cycle sound of this switch: low for a return to the Original, else high.
    #[must_use]
    pub fn sound(self) -> Sound {
        match self {
            Self::ToOriginal => Sound::LowTone,
            Self::ToVariant => Sound::HighTone,
        }
    }
}

const TONE_MS: u32 = 80;
const TONE_VOLUME: f64 = 0.2;
const TONE_ATTACK: f64 = 0.005;
const HIGH_HZ: f64 = 880.;
/// D5, a fifth under the high tone.
const LOW_HZ: f64 = 587.33;
const EXPLOSION_MS: u32 = 300;
const EXPLOSION_VOLUME: f64 = 0.45;
const EXPLOSION_ATTACK: f64 = 0.002;
const THUMP_HZ: f64 = 55.;

impl Sound {
    /// Mono samples in -1..=1 at `sample_rate` Hz. They start and end at silence, so
    /// they do not click.
    #[must_use]
    pub fn samples(self, sample_rate: u32) -> Vec<f32> {
        match self {
            Self::HighTone => tone(HIGH_HZ, sample_rate),
            Self::LowTone => tone(LOW_HZ, sample_rate),
            Self::Explosion => explosion(sample_rate),
        }
    }
}

/// Sample indexes of `ms` milliseconds, each with its time in seconds and its part
/// (0..1) of the sound.
fn timeline(ms: u32, sample_rate: u32) -> impl Iterator<Item = (f64, f64)> {
    let count = sample_rate.saturating_mul(ms) / 1000;
    let rate = f64::from(sample_rate);
    (0..count).map(move |index| {
        let index = f64::from(index);
        (index / rate, index / f64::from(count))
    })
}

fn tone(hz: f64, sample_rate: u32) -> Vec<f32> {
    timeline(TONE_MS, sample_rate)
        .map(|(time, part)| {
            let envelope = (time / TONE_ATTACK).min(1.) * (1. - part).powi(2);
            to_f32(TONE_VOLUME * envelope * (TAU * hz * time).sin())
        })
        .collect()
}

/// Low-passed noise that gets darker as it fades, over a low thump.
fn explosion(sample_rate: u32) -> Vec<f32> {
    let mut seed: u32 = 0x9E37_79B9;
    let mut rumble = 0.;
    timeline(EXPLOSION_MS, sample_rate)
        .map(|(time, part)| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = f64::from(seed) / f64::from(u32::MAX) * 2. - 1.;
            rumble += (0.5 - 0.45 * part) * (noise - rumble);
            let thump = (TAU * THUMP_HZ * time).sin() * (1. - part).powi(4);
            let envelope = (time / EXPLOSION_ATTACK).min(1.) * (1. - part).powi(3);
            let sample = EXPLOSION_VOLUME * (envelope * rumble + 0.6 * thump);
            to_f32(sample.clamp(-EXPLOSION_VOLUME, EXPLOSION_VOLUME))
        })
        .collect()
}

#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "audio samples need no f64 precision; f32 has no From<f64>"
)]
fn to_f32(sample: f64) -> f32 {
    sample as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn peak(samples: &[f32]) -> f32 {
        samples
            .iter()
            .fold(0., |peak, sample| peak.max(sample.abs()))
    }

    fn zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| matches!(pair, [a, b] if (*a < 0.) != (*b < 0.)))
            .count()
    }

    #[test]
    fn a_return_to_the_original_plays_the_low_tone() {
        assert_eq!(Switch::ToOriginal.sound(), Sound::LowTone);
        assert_eq!(Switch::ToVariant.sound(), Sound::HighTone);
    }

    #[test]
    fn tones_are_80_ms_quiet_and_start_and_end_at_silence() {
        for sound in [Sound::HighTone, Sound::LowTone] {
            let samples = sound.samples(RATE);
            assert_eq!(samples.len(), 3840, "{sound:?}");
            assert!(peak(&samples) <= 0.25, "{sound:?}");
            assert!(samples.first().is_some_and(|first| first.abs() < 0.01));
            assert!(samples.last().is_some_and(|last| last.abs() < 0.01));
        }
    }

    #[test]
    fn the_high_tone_is_880_hz_and_the_low_tone_587_hz() {
        // 80 ms of 880 Hz crosses zero about 141 times; of 587 Hz, about 94 times.
        assert!((138..=143).contains(&zero_crossings(&Sound::HighTone.samples(RATE))));
        assert!((91..=96).contains(&zero_crossings(&Sound::LowTone.samples(RATE))));
    }

    #[test]
    fn the_explosion_is_300_ms_and_ends_at_silence() {
        let samples = Sound::Explosion.samples(RATE);
        assert_eq!(samples.len(), 14_400);
        assert!(peak(&samples) <= 0.5);
        assert!(peak(&samples) >= 0.1);
        assert!(samples.last().is_some_and(|last| last.abs() < 0.01));
    }

    #[test]
    fn no_sample_rate_gives_no_samples() {
        assert_eq!(Sound::HighTone.samples(0), Vec::<f32>::new());
    }
}
