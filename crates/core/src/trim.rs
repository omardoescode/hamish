use std::fmt;
use std::ops::Range;

use crate::{Document, RangeError};

/// How much a Trim cuts (FR-55).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrimLevel {
    /// No Trim: the text as written.
    Original,
    SlightTrim,
    TightenMore,
    EvenSharper,
    CutInHalf,
}

impl TrimLevel {
    /// The levels in the order the Lab lists them (FR-55).
    pub const ALL: [Self; 5] = [
        Self::Original,
        Self::SlightTrim,
        Self::TightenMore,
        Self::EvenSharper,
        Self::CutInHalf,
    ];

    /// About how many words to cut, in percent of the words.
    #[must_use]
    pub fn target_percent(self) -> u8 {
        match self {
            Self::Original => 0,
            Self::SlightTrim => 10,
            Self::TightenMore => 20,
            Self::EvenSharper => 30,
            Self::CutInHalf => 50,
        }
    }
}

/// What the AI gets for a Trim: the level and the whole active text (FR-56).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimRequest {
    level: TrimLevel,
    text: String,
}

impl TrimRequest {
    #[must_use]
    pub fn level(&self) -> TrimLevel {
        self.level
    }

    /// The active text of the whole Document when the request was made.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One range a Trim marks for cut. `range` is a byte range of the active text.
/// A kept cut has left the proposal: its text stays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimCut {
    pub range: Range<usize>,
    pub kept: bool,
}

/// The ranges a Trim marks for cut, with word counts before and after (FR-57).
/// The text does not change until the cuts are made. Its fades are not Ghost ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimProposal {
    level: TrimLevel,
    cuts: Vec<TrimCut>,
    words_before: usize,
    words_after: usize,
}

impl TrimProposal {
    #[must_use]
    pub fn level(&self) -> TrimLevel {
        self.level
    }

    /// The cuts in document order.
    #[must_use]
    pub fn cuts(&self) -> &[TrimCut] {
        &self.cuts
    }

    /// Words of the Document when the proposal was made (Counter rules).
    #[must_use]
    pub fn words_before(&self) -> usize {
        self.words_before
    }

    /// Words of the Document without the cuts still in the proposal (Counter rules).
    #[must_use]
    pub fn words_after(&self) -> usize {
        self.words_after
    }

    /// The change from words before to words after, in percent, rounded half away from zero.
    #[must_use]
    pub fn percent(&self) -> i64 {
        let before = i64::try_from(self.words_before).unwrap_or(i64::MAX);
        let after = i64::try_from(self.words_after).unwrap_or(i64::MAX);
        if before == 0 {
            return 0;
        }
        let change = 200 * (after - before);
        (change + change.signum() * before) / (2 * before)
    }

    fn open_ranges(&self) -> Vec<Range<usize>> {
        self.cuts
            .iter()
            .filter(|cut| !cut.kept)
            .map(|cut| cut.range.clone())
            .collect()
    }
}

impl fmt::Display for TrimProposal {
    /// `535 → 480 words · -10%`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = if self.words_after == 1 {
            "word"
        } else {
            "words"
        };
        let sign = if self.percent() > 0 { "+" } else { "" };
        write!(
            f,
            "{} → {} {words} · {sign}{}%",
            self.words_before,
            self.words_after,
            self.percent()
        )
    }
}

/// Why a Trim operation was refused. A refused operation changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrimError {
    /// The text is not the request text any more.
    Stale,
    NoProposal,
    /// The proposal has no cut at this index.
    NoCut(usize),
    Range(RangeError),
}

impl From<RangeError> for TrimError {
    fn from(error: RangeError) -> Self {
        Self::Range(error)
    }
}

impl fmt::Display for TrimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale => f.write_str("the text changed since the Trim request"),
            Self::NoProposal => f.write_str("no Trim proposal is open"),
            Self::NoCut(index) => write!(f, "the Trim proposal has no cut {index}"),
            Self::Range(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for TrimError {}

impl Document {
    /// A Trim request at `level` over the whole active text. `None` for
    /// [`TrimLevel::Original`]: it needs no AI; [`Document::close_trim`] shows the text as written.
    #[must_use]
    pub fn trim_request(&self, level: TrimLevel) -> Option<TrimRequest> {
        (level != TrimLevel::Original).then(|| TrimRequest {
            level,
            text: self.text().to_owned(),
        })
    }

    /// Makes `cuts` the Trim proposal, in place of any open one. The text stays (FR-56).
    /// A cut with an empty range or one that is not a valid range of the text is dropped;
    /// cuts that overlap join. Not an undo step: the proposal changes no text.
    ///
    /// # Errors
    /// [`TrimError::Stale`] when the text is not the request text any more.
    pub fn apply_trim_result(
        &mut self,
        request: &TrimRequest,
        cuts: &[Range<usize>],
    ) -> Result<(), TrimError> {
        if self.text() != request.text {
            return Err(TrimError::Stale);
        }
        let mut valid: Vec<Range<usize>> = cuts
            .iter()
            .filter(|cut| !cut.is_empty() && self.text().get((*cut).clone()).is_some())
            .cloned()
            .collect();
        valid.sort_by_key(|cut| cut.start);
        let mut joined: Vec<Range<usize>> = Vec::new();
        for cut in valid {
            match joined.last_mut() {
                Some(last) if cut.start < last.end => last.end = last.end.max(cut.end),
                _ => joined.push(cut),
            }
        }
        let mut proposal = TrimProposal {
            level: request.level,
            cuts: joined
                .into_iter()
                .map(|range| TrimCut { range, kept: false })
                .collect(),
            words_before: self.counter().words,
            words_after: 0,
        };
        self.recount(&mut proposal);
        self.trim = Some(proposal);
        Ok(())
    }

    /// The open Trim proposal.
    #[must_use]
    pub fn trim_proposal(&self) -> Option<&TrimProposal> {
        self.trim.as_ref()
    }

    /// Closes the Trim proposal (Done, or the Original level). The text stays.
    pub fn close_trim(&mut self) {
        self.trim = None;
    }

    /// Keep: cut `index` leaves the proposal, its text stays, and the counts update (FR-58).
    ///
    /// # Errors
    /// [`TrimError::NoProposal`] when no proposal is open, [`TrimError::NoCut`] when it has
    /// no cut `index`.
    pub fn keep_trim_cut(&mut self, index: usize) -> Result<(), TrimError> {
        let mut proposal = self.trim.take().ok_or(TrimError::NoProposal)?;
        let Some(cut) = proposal.cuts.get_mut(index) else {
            self.trim = Some(proposal);
            return Err(TrimError::NoCut(index));
        };
        cut.kept = true;
        self.recount(&mut proposal);
        self.trim = Some(proposal);
        Ok(())
    }

    /// Make the cuts: deletes the cuts still in the proposal as one undoable writer edit,
    /// and closes the proposal (FR-59).
    ///
    /// # Errors
    /// [`TrimError::NoProposal`] when no proposal is open.
    pub fn make_trim_cuts(&mut self) -> Result<(), TrimError> {
        let proposal = self.trim.take().ok_or(TrimError::NoProposal)?;
        match self.delete_ranges(&proposal.open_ranges()) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.trim = Some(proposal);
                Err(error.into())
            }
        }
    }

    fn recount(&self, proposal: &mut TrimProposal) {
        let all = 0..self.text().len();
        proposal.words_after = self.count_outside(all, &proposal.open_ranges()).words;
    }
}
