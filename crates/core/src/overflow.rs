//! Overflow: the free text near the Document, its blocks, Stash, and block use.

use std::fmt;
use std::ops::Range;

use crate::RangeError;

/// The free text the writer keeps near the Document, outside the main text.
/// Blank lines (empty or whitespace only) split it into blocks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Overflow {
    text: String,
}

/// Why an Overflow operation was refused. A refused operation changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowError {
    Range(RangeError),
    EmptyRange,
    /// No block holds this Overflow offset.
    NoBlock(usize),
}

impl fmt::Display for OverflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range(error) => error.fmt(f),
            Self::EmptyRange => f.write_str("a Stash needs a non-empty range"),
            Self::NoBlock(at) => write!(f, "no Overflow block at {at}"),
        }
    }
}

impl std::error::Error for OverflowError {}

impl From<RangeError> for OverflowError {
    fn from(error: RangeError) -> Self {
        Self::Range(error)
    }
}

impl Overflow {
    pub(crate) fn new(text: String) -> Self {
        Self { text }
    }

    /// The whole Overflow text, blank lines included.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The blocks in order, as byte ranges of [`Overflow::text`]: each a run of lines that
    /// are not blank, from the start of its first line to the end of its last, newline excluded.
    #[must_use]
    pub fn blocks(&self) -> Vec<Range<usize>> {
        let mut blocks: Vec<Range<usize>> = Vec::new();
        let mut open = false;
        let mut start = 0;
        for line in self.text.split_inclusive('\n') {
            let content = line.trim_end_matches(['\n', '\r']);
            let range = start..start + content.len();
            start += line.len();
            let blank = content.trim().is_empty();
            match (blank, open, blocks.last_mut()) {
                (true, ..) => open = false,
                (false, true, Some(block)) => block.end = range.end,
                (false, ..) => {
                    blocks.push(range);
                    open = true;
                }
            }
        }
        blocks
    }

    /// The text of the block that holds offset `at`, on its end included.
    pub(crate) fn block_at(&self, at: usize) -> Option<&str> {
        self.blocks()
            .into_iter()
            .find(|block| block.start <= at && at <= block.end)
            .and_then(|block| self.text.get(block))
    }

    pub(crate) fn text_mut(&mut self) -> &mut String {
        &mut self.text
    }

    /// Appends `text` at the end, after a blank line when there is text before it.
    pub(crate) fn append(&mut self, text: &str) {
        let separator = match () {
            () if self.text.is_empty() || self.text.ends_with("\n\n") => "",
            () if self.text.ends_with('\n') => "\n",
            () => "\n\n",
        };
        self.text.push_str(separator);
        self.text.push_str(text);
    }
}
