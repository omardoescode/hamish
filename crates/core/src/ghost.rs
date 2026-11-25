use std::fmt;
use std::ops::Range;

use crate::RangeError;

/// Why a Ghost or Revive was refused. A refused operation changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhostError {
    Range(RangeError),
    EmptyRange,
    /// The range partly overlaps a Slot (strict nesting, ADR 0006).
    PartialOverlap,
    /// No visible Ghost range touches this range.
    NoGhost(Range<usize>),
}

impl fmt::Display for GhostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range(error) => error.fmt(f),
            Self::EmptyRange => f.write_str("a Ghost needs a non-empty range"),
            Self::PartialOverlap => f.write_str("the range partly overlaps a Slot"),
            Self::NoGhost(range) => write!(f, "no Ghost range at {}..{}", range.start, range.end),
        }
    }
}

impl std::error::Error for GhostError {}

impl From<RangeError> for GhostError {
    fn from(error: RangeError) -> Self {
        Self::Range(error)
    }
}

/// Adds `range` to `ghosts`; Ghost ranges that overlap or touch it join it.
pub(crate) fn join(ghosts: &mut Vec<Range<usize>>, range: Range<usize>) {
    let (touching, mut rest): (Vec<_>, Vec<_>) = std::mem::take(ghosts)
        .into_iter()
        .partition(|ghost| ghost.start <= range.end && range.start <= ghost.end);
    let joined = touching.into_iter().fold(range, |all, ghost| {
        all.start.min(ghost.start)..all.end.max(ghost.end)
    });
    rest.push(joined);
    rest.sort_by_key(|ghost| ghost.start);
    *ghosts = rest;
}

/// Removes the Ghost ranges that `range` touches. An empty `range` touches a Ghost
/// range it lies in or on an edge of. Returns how many it removed.
pub(crate) fn revive(ghosts: &mut Vec<Range<usize>>, range: &Range<usize>) -> usize {
    let before = ghosts.len();
    ghosts.retain(|ghost| !touches(ghost, range));
    before - ghosts.len()
}

fn touches(ghost: &Range<usize>, range: &Range<usize>) -> bool {
    if range.is_empty() {
        return ghost.start <= range.start && range.start <= ghost.end;
    }
    ghost.start < range.end && range.start < ghost.end
}
