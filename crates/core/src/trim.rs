//! Trim: Trim levels, requests, the Trim proposal, Keep, Make the cuts, and Walk through.

use std::fmt;
use std::ops::Range;

use crate::{Document, RangeError};

/// How much a Trim cuts.
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
    /// The levels in the order the Lab lists them.
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

/// What the AI gets for a Trim: the level and the whole active text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimRequest {
    level: TrimLevel,
    text: String,
}

impl TrimRequest {
    /// The Trim level to run.
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

impl TrimCut {
    /// An insert at an edge goes outside the cut. `None` when the edit changes its text.
    fn follow(mut self, edit: &Range<usize>, inserted: usize) -> Option<Self> {
        if edit.end <= self.range.start {
            self.range =
                self.range.start + inserted - edit.len()..self.range.end + inserted - edit.len();
            return Some(self);
        }
        (edit.start >= self.range.end).then_some(self)
    }
}

/// The ranges a Trim marks for cut, with word counts before and after.
/// The text does not change until the cuts are made. Its fades are not Ghost ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimProposal {
    level: TrimLevel,
    cuts: Vec<TrimCut>,
    words_before: usize,
    words_after: usize,
    current: Option<usize>,
}

impl TrimProposal {
    /// The index of the cut under the Walk through cursor. `None` when no walk is on or it
    /// went past the last cut.
    #[must_use]
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// The first cut still in the proposal at or after index `from`.
    fn next_open(&self, from: usize) -> Option<usize> {
        self.cuts
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, cut)| !cut.kept)
            .map(|(index, _)| index)
    }

    /// The Trim level that made the proposal.
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

    /// This proposal after a text edit: `edit` was replaced by `inserted` bytes. Cuts after
    /// the edit move; a cut whose text the edit changes leaves the proposal.
    fn follow(mut self, edit: &Range<usize>, inserted: usize) -> Self {
        if edit.is_empty() && inserted == 0 {
            return self;
        }
        let followed: Vec<Option<TrimCut>> = self
            .cuts
            .into_iter()
            .map(|cut| cut.follow(edit, inserted))
            .collect();
        let gone_before = |index: usize| {
            followed
                .iter()
                .take(index)
                .filter(|cut| cut.is_none())
                .count()
        };
        let current = self.current.map(|index| index - gone_before(index));
        self.cuts = followed.into_iter().flatten().collect();
        self.current = current.and_then(|index| self.next_open(index));
        self
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
    /// No Walk through is on, or it went past the last cut.
    NotWalking,
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
            Self::NotWalking => f.write_str("no Walk through is on"),
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

    /// Makes `cuts` the Trim proposal, in place of any open one. The text stays.
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
            current: None,
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

    /// Keep: cut `index` leaves the proposal, its text stays, and the counts update.
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
        proposal.current = proposal
            .current
            .and_then(|current| proposal.next_open(current));
        self.recount(&mut proposal);
        self.trim = Some(proposal);
        Ok(())
    }

    /// Make the cuts: deletes the cuts still in the proposal as one undoable writer edit,
    /// and closes the proposal.
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

    /// Walk through: puts the cursor on the first cut still in the proposal.
    ///
    /// # Errors
    /// [`TrimError::NoProposal`] when no proposal is open.
    pub fn walk_trim(&mut self) -> Result<(), TrimError> {
        let proposal = self.trim.as_mut().ok_or(TrimError::NoProposal)?;
        proposal.current = proposal.next_open(0);
        Ok(())
    }

    /// Walk through: keeps the cut under the cursor and goes to the next one.
    ///
    /// # Errors
    /// [`TrimError::NoProposal`] when no proposal is open, [`TrimError::NotWalking`] when
    /// no cut is under the cursor.
    pub fn walk_keep(&mut self) -> Result<(), TrimError> {
        let proposal = self.trim.as_ref().ok_or(TrimError::NoProposal)?;
        let current = proposal.current.ok_or(TrimError::NotWalking)?;
        self.keep_trim_cut(current)
    }

    /// Walk through: deletes the text of the cut under the cursor as one undoable writer
    /// edit, and goes to the next cut.
    ///
    /// # Errors
    /// [`TrimError::NoProposal`] when no proposal is open, [`TrimError::NotWalking`] when
    /// no cut is under the cursor.
    pub fn walk_cut(&mut self) -> Result<(), TrimError> {
        let proposal = self.trim.as_ref().ok_or(TrimError::NoProposal)?;
        let range = proposal
            .current
            .and_then(|current| proposal.cuts.get(current))
            .map(|cut| cut.range.clone())
            .ok_or(TrimError::NotWalking)?;
        Ok(self.delete(range)?)
    }

    /// The Trim proposal after a text edit: `edit` was replaced by `inserted` bytes.
    pub(crate) fn follow_trim(&mut self, edit: &Range<usize>, inserted: usize) {
        let Some(proposal) = self.trim.take() else {
            return;
        };
        let mut proposal = proposal.follow(edit, inserted);
        self.recount(&mut proposal);
        self.trim = Some(proposal);
    }

    fn recount(&self, proposal: &mut TrimProposal) {
        let all = 0..self.text().len();
        proposal.words_after = self.count_outside(all, &proposal.open_ranges()).words;
    }
}
