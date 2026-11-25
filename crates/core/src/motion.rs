//! Effect timing: the one source of Effect durations and progress, Reduced motion included.
//! Views turn progress into a look.

use std::time::Duration;

/// A timed visual effect. Effects never block input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// New Active variant text fades in.
    VariantFadeIn,
    /// Burst over a deleted Variant.
    Explosion,
    /// Burst when an undo puts a deleted Variant back; shorter than the explosion.
    UndoBurst,
    /// The paragraph with the caret zooms in, the one it left zooms out.
    FocusZoom,
    /// One round of the "??" dance of a quick fill. It repeats while the AI works; with reduced motion the "??" stays still.
    QuickFillDance,
}

impl Effect {
    /// How long the effect runs with full motion.
    #[must_use]
    pub const fn duration(self) -> Duration {
        match self {
            Self::Explosion => Duration::from_millis(650),
            Self::UndoBurst => Duration::from_millis(450),
            Self::VariantFadeIn | Self::FocusZoom => Duration::from_millis(150),
            Self::QuickFillDance => Duration::from_millis(1300),
        }
    }

    /// How far the effect is `elapsed` after its start: 0.0 at the start, 1.0 when done.
    /// With reduced motion an effect is done at once.
    ///
    /// ```
    /// use std::time::Duration;
    /// use hamish_core::Effect;
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
