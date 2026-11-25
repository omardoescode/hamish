use std::fmt;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::{Document, RangeError};

/// A Lab check: a rule the AI marks matching text for (FR-51).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabCheck {
    /// Fix punctuation and typos: marks carry a proposed fix (FR-53).
    PunctuationTypos,
    WeakestSentences,
    LongSentences,
    ConvolutedSentences,
    OffToneWords,
    HedgesFiller,
}

impl LabCheck {
    /// The checks in the order the Lab lists them (FR-51).
    pub const ALL: [Self; 6] = [
        Self::PunctuationTypos,
        Self::WeakestSentences,
        Self::LongSentences,
        Self::ConvolutedSentences,
        Self::OffToneWords,
        Self::HedgesFiller,
    ];
}

/// What the AI gets for a Lab check: the check and the whole active text (FR-52).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabRequest {
    check: LabCheck,
    text: String,
}

impl LabRequest {
    #[must_use]
    pub fn check(&self) -> LabCheck {
        self.check
    }

    /// The active text of the whole Document when the request was made.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One range the AI marks for a Lab check. `range` is a byte range of the request text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabFinding {
    pub range: Range<usize>,
    /// Why the text matches the check.
    pub note: Option<String>,
    /// Text to put in place of the range. Kept for [`LabCheck::PunctuationTypos`] only.
    pub fix: Option<String>,
}

/// Stable name of a Lab mark for the life of its Document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LabMarkId(pub(crate) u64);

impl LabMarkId {
    pub(crate) fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for LabMarkId {
    fn default() -> Self {
        Self(1)
    }
}

/// An open mark from a Lab check. `range` is a byte range of the active text.
/// Accept or dismiss ends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabMark {
    pub id: LabMarkId,
    pub check: LabCheck,
    pub range: Range<usize>,
    pub note: Option<String>,
    pub fix: Option<String>,
}

impl LabMark {
    /// This mark after a text edit: `edit` was replaced by `inserted` bytes.
    /// An insert at an edge goes outside the mark; an empty edit (a Ghost, a new Slot)
    /// changes nothing. `None` when the mark ends: all its text was removed, or it has a
    /// proposed fix and its text changed (the fix is stale).
    pub(crate) fn follow(mut self, edit: &Range<usize>, inserted: usize) -> Option<Self> {
        if edit.is_empty() && inserted == 0 {
            return Some(self);
        }
        let range = self.range.clone();
        if edit.end <= range.start {
            self.range = range.start + inserted - edit.len()..range.end + inserted - edit.len();
            return Some(self);
        }
        if edit.start >= range.end {
            return Some(self);
        }
        let covered = edit.start <= range.start && range.end <= edit.end;
        if covered || self.fix.is_some() {
            return None;
        }
        let start = if edit.start < range.start {
            edit.start + inserted
        } else {
            range.start
        };
        let end = if edit.end < range.end {
            range.end + inserted - edit.len()
        } else {
            edit.start
        };
        self.range = start..end;
        Some(self)
    }
}

/// Why a Lab operation was refused. A refused operation changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabError {
    /// The text is not the request text any more.
    Stale,
    NoMark(LabMarkId),
    /// The mark has no proposed fix to accept.
    NoFix(LabMarkId),
    Range(RangeError),
}

impl fmt::Display for LabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale => f.write_str("the text changed since the Lab check"),
            Self::NoMark(id) => write!(f, "no Lab mark {}", id.0),
            Self::NoFix(id) => write!(f, "Lab mark {} has no proposed fix", id.0),
            Self::Range(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for LabError {}

impl From<RangeError> for LabError {
    fn from(error: RangeError) -> Self {
        Self::Range(error)
    }
}

impl Document {
    /// A Lab request for `check` over the whole active text.
    #[must_use]
    pub fn lab_request(&self, check: LabCheck) -> LabRequest {
        LabRequest {
            check,
            text: self.text().to_owned(),
        }
    }

    /// Marks the ranges of `findings` as one undoable step. The text stays (FR-52).
    /// The open marks of the same check end. A finding with an empty range or one that is
    /// not a valid range of the text is dropped. Returns the new marks in `findings` order.
    ///
    /// # Errors
    /// [`LabError::Stale`] when the text is not the request text any more.
    pub fn apply_lab_result(
        &mut self,
        request: &LabRequest,
        findings: &[LabFinding],
    ) -> Result<Vec<LabMarkId>, LabError> {
        if self.text() != request.text {
            return Err(LabError::Stale);
        }
        let mut marks: Vec<LabMark> = self
            .lab_marks()
            .iter()
            .filter(|mark| mark.check != request.check)
            .cloned()
            .collect();
        let mut ids = Vec::new();
        let valid = findings.iter().filter(|finding| {
            !finding.range.is_empty() && self.text().get(finding.range.clone()).is_some()
        });
        for finding in valid.collect::<Vec<_>>() {
            let id = self.new_lab_mark_id();
            marks.push(LabMark {
                id,
                check: request.check,
                range: finding.range.clone(),
                note: finding.note.clone(),
                fix: finding
                    .fix
                    .clone()
                    .filter(|_| request.check == LabCheck::PunctuationTypos),
            });
            ids.push(id);
        }
        marks.sort_by_key(|mark| mark.range.start);
        self.commit_lab_marks(marks)?;
        Ok(ids)
    }

    /// Ends Lab mark `id` as one undoable step. The text stays.
    ///
    /// # Errors
    /// [`LabError::NoMark`] when `id` is not an open mark.
    pub fn dismiss_lab_mark(&mut self, id: LabMarkId) -> Result<(), LabError> {
        if !self.lab_marks().iter().any(|mark| mark.id == id) {
            return Err(LabError::NoMark(id));
        }
        let marks = self
            .lab_marks()
            .iter()
            .filter(|mark| mark.id != id)
            .cloned()
            .collect();
        Ok(self.commit_lab_marks(marks)?)
    }

    /// Puts the proposed fix of Lab mark `id` in place of its text, as one undoable writer
    /// edit. The mark ends with its text.
    ///
    /// # Errors
    /// [`LabError::NoMark`] when `id` is not an open mark, [`LabError::NoFix`] when it has
    /// no proposed fix.
    pub fn accept_lab_fix(&mut self, id: LabMarkId) -> Result<(), LabError> {
        let mark = self
            .lab_marks()
            .iter()
            .find(|mark| mark.id == id)
            .ok_or(LabError::NoMark(id))?;
        let fix = mark.fix.clone().ok_or(LabError::NoFix(id))?;
        Ok(self.replace(mark.range.clone(), &fix)?)
    }
}
