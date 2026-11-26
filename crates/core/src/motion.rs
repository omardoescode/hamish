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
    /// One on and off round of a caret blink. It repeats; with reduced motion the caret stays on.
    CaretBlink,
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
            Self::CaretBlink => Duration::from_millis(1000),
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

/// A blinking caret shows `elapsed` after its blink starts: on in the first half of each
/// [`Effect::CaretBlink`] round, off in the second half. With reduced motion it stays on.
#[must_use]
pub fn caret_shown(elapsed: Duration, reduced_motion: bool) -> bool {
    let round = Effect::CaretBlink.duration().as_millis();
    reduced_motion || elapsed.as_millis() % round < round / 2
}

/// How long after `elapsed` a blinking caret next turns on or off: at each half of an
/// [`Effect::CaretBlink`] round, counted from the start of its blink.
#[must_use]
pub fn next_caret_toggle(elapsed: Duration) -> Duration {
    let half = Effect::CaretBlink.duration() / 2;
    let into_half = elapsed.as_nanos() % half.as_nanos();
    half.saturating_sub(Duration::from_nanos(u64::try_from(into_half).unwrap_or(0)))
}
