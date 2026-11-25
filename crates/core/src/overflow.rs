use std::fmt;

use crate::RangeError;

/// The free text the writer keeps near the Document, outside the main text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Overflow {
    text: String,
}

/// Why an Overflow operation was refused. A refused operation changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowError {
    Range(RangeError),
    EmptyRange,
}

impl fmt::Display for OverflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range(error) => error.fmt(f),
            Self::EmptyRange => f.write_str("a Stash needs a non-empty range"),
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
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
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
