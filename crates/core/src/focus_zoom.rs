//! Focus zoom: the paragraph with the caret shows a little larger, to pull
//! attention to it; the opposite of Ghost. A paragraph is one line of the text (between
//! newlines). The zoom is a look only: the text keeps its layout.

use std::ops::Range;
use std::time::Instant;

use crate::Effect;

/// How much larger the focused paragraph shows.
pub const FOCUS_ZOOM_SCALE: f32 = 1.02;

/// Which paragraphs show zoomed, and how far each one is in its zoom in or out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FocusZoom {
    focused: Option<Change>,
    leaving: Option<Change>,
}

/// One paragraph on its way from one scale to another.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Change {
    paragraph: usize,
    from: f32,
    to: f32,
    started: Instant,
}

impl Change {
    fn progress(&self, now: Instant, reduced_motion: bool) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        Effect::FocusZoom.progress(elapsed, reduced_motion)
    }

    /// Eased in and out (smoothstep).
    fn scale(&self, now: Instant, reduced_motion: bool) -> f32 {
        let progress = self.progress(now, reduced_motion);
        let eased = progress * progress * (3. - 2. * progress);
        self.from + (self.to - self.from) * eased
    }
}

impl FocusZoom {
    /// Zooms `paragraph` (an index, see [`paragraph_index`]) in from `now` and the one
    /// focused before out; `None` zooms all out. The same paragraph again changes nothing.
    pub fn focus(&mut self, paragraph: Option<usize>, now: Instant, reduced_motion: bool) {
        if self.focused.map(|change| change.paragraph) == paragraph {
            return;
        }
        let from = paragraph.map_or(1., |index| self.scale(index, now, reduced_motion));
        let old = self.focused.take();
        self.leaving = old.map(|change| Change {
            paragraph: change.paragraph,
            from: change.scale(now, reduced_motion),
            to: 1.,
            started: now,
        });
        self.focused = paragraph.map(|index| Change {
            paragraph: index,
            from,
            to: FOCUS_ZOOM_SCALE,
            started: now,
        });
    }

    /// The scale of `paragraph` at `now`: 1.0 when it shows at its normal size.
    #[must_use]
    pub fn scale(&self, paragraph: usize, now: Instant, reduced_motion: bool) -> f32 {
        [self.focused, self.leaving]
            .into_iter()
            .flatten()
            .find(|change| change.paragraph == paragraph)
            .map_or(1., |change| change.scale(now, reduced_motion))
    }

    /// Each paragraph not at its normal size at `now`, with its scale; the focused one first.
    #[must_use]
    pub fn zoomed(&self, now: Instant, reduced_motion: bool) -> Vec<(usize, f32)> {
        [self.focused, self.leaving]
            .into_iter()
            .flatten()
            .map(|change| (change.paragraph, change.scale(now, reduced_motion)))
            .filter(|&(_, scale)| (scale - 1.).abs() > f32::EPSILON)
            .collect()
    }

    /// A zoom in or out still runs at `now`: the view needs the next frame.
    #[must_use]
    pub fn is_moving(&self, now: Instant, reduced_motion: bool) -> bool {
        [self.focused, self.leaving]
            .into_iter()
            .flatten()
            .any(|change| change.progress(now, reduced_motion) < 1.)
    }
}

/// The index of the paragraph (line) that holds byte `offset` of `text`.
#[must_use]
pub fn paragraph_index(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(0, |before| before.matches('\n').count())
}

/// The byte range of paragraph `index` of `text`, without its newline; `None` past the
/// last one.
#[must_use]
pub fn paragraph_range(text: &str, index: usize) -> Option<Range<usize>> {
    let mut start = 0;
    for (at, line) in text.split('\n').enumerate() {
        if at == index {
            return Some(start..start + line.len());
        }
        start += line.len() + 1;
    }
    None
}
