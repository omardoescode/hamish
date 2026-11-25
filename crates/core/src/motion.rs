//! Animation timing (spec NFRs "Animation timing" and "Reduced motion"): the one source of
//! effect durations and progress. Views turn progress into a look.

use std::time::Duration;

/// A timed visual effect. Effects never block input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// New Active variant text fades in (FR-81; spec: 100 to 200 ms).
    VariantFadeIn,
    /// Burst over a deleted Variant (FR-78; spec: 500 to 800 ms).
    Explosion,
}

impl Effect {
    /// How long the effect runs with full motion.
    #[must_use]
    pub const fn duration(self) -> Duration {
        match self {
            Self::VariantFadeIn => Duration::from_millis(150),
            Self::Explosion => Duration::from_millis(650),
        }
    }

    /// How far the effect is `elapsed` after its start: 0.0 at the start, 1.0 when done.
    /// With reduced motion an effect is done at once.
    ///
    /// ```
    /// use std::time::Duration;
    /// use ka_teb_core::Effect;
    ///
    /// let half = Effect::VariantFadeIn.progress(Duration::from_millis(75), false);
    /// assert!((half - 0.5).abs() < 1e-6);
    /// ```
    #[must_use]
    pub fn progress(self, elapsed: Duration, reduced_motion: bool) -> f32 {
        if reduced_motion {
            return 1.0;
        }
        (elapsed.as_secs_f32() / self.duration().as_secs_f32()).min(1.0)
    }
}
