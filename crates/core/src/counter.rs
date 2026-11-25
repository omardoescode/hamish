use std::fmt;

/// Live word and character count of the Document, or of the selection.
///
/// Words are runs of non-whitespace. Chars are Unicode scalar values, whitespace included.
///
/// ```
/// use ka_teb_core::Counter;
///
/// assert_eq!(Counter::of("Hello, dear world").to_string(), "3 words 17 chars");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counter {
    pub words: usize,
    pub chars: usize,
}

impl Counter {
    #[must_use]
    pub fn of(text: &str) -> Self {
        Self {
            words: text.split_whitespace().count(),
            chars: text.chars().count(),
        }
    }
}

impl fmt::Display for Counter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} words {} chars", self.words, self.chars)
    }
}
