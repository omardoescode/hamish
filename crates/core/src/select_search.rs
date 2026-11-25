//! The search of an open Settings select: the query and the row the keyboard is on.

/// The query typed in an open select, and the row the keyboard is on (an index into the
/// matches).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectSearch {
    query: String,
    row: usize,
}

impl SelectSearch {
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The keyboard row, an index into the matches. It is 0 after each query change.
    #[must_use]
    pub fn row(&self) -> usize {
        self.row
    }

    /// Adds `text` at the end of the query.
    pub fn type_text(&mut self, text: &str) {
        self.query.push_str(text);
        self.row = 0;
    }

    /// Removes the last character of the query.
    pub fn delete_backward(&mut self) {
        self.query.pop();
        self.row = 0;
    }

    /// The indexes of the `names` that contain the query, in order. Case does not matter.
    /// An empty query matches all names.
    pub fn matches<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
        let query = self.query.to_lowercase();
        names
            .into_iter()
            .enumerate()
            .filter(|(_, name)| name.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    /// Moves the keyboard row one up; it stops at the first match.
    pub fn move_up(&mut self) {
        self.row = self.row.saturating_sub(1);
    }

    /// Moves the keyboard row one down; it stops at the last of `count` matches.
    pub fn move_down(&mut self, count: usize) {
        self.row = (self.row + 1).min(count.saturating_sub(1));
    }
}
