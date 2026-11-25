use std::fmt;
use std::ops::Range;

use crate::Counter;

/// One piece of writing and its edit history.
#[derive(Debug, Clone, Default)]
pub struct Document {
    text: String,
    undo_steps: Vec<Step>,
    redo_steps: Vec<Step>,
}

/// One undoable change: `removed` at `at` was swapped for `inserted`.
#[derive(Debug, Clone)]
struct Step {
    at: usize,
    removed: String,
    inserted: String,
}

impl Step {
    fn inverse(self) -> Self {
        Self {
            at: self.at,
            removed: self.inserted,
            inserted: self.removed,
        }
    }
}

/// A byte range or offset that is out of bounds or not on a char boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeError {
    pub range: Range<usize>,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid text range {}..{}",
            self.range.start, self.range.end
        )
    }
}

impl std::error::Error for RangeError {}

impl Document {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn counter(&self) -> Counter {
        Counter::of(&self.text)
    }

    /// The first line, shown as the title (FR-5). `None` when the first line is blank.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.text
            .lines()
            .next()
            .filter(|line| !line.trim().is_empty())
    }

    /// Counter of the text in byte range `selection`.
    ///
    /// # Errors
    /// [`RangeError`] when `selection` is out of bounds, reversed, or not on char boundaries.
    pub fn selection_counter(&self, selection: Range<usize>) -> Result<Counter, RangeError> {
        self.text
            .get(selection.clone())
            .map(Counter::of)
            .ok_or(RangeError { range: selection })
    }

    /// Inserts `text` at byte offset `at`.
    ///
    /// # Errors
    /// [`RangeError`] when `at` is out of bounds or not on a char boundary.
    pub fn insert(&mut self, at: usize, text: &str) -> Result<(), RangeError> {
        self.replace(at..at, text)
    }

    /// Removes the text in byte range `range`.
    ///
    /// # Errors
    /// [`RangeError`] when `range` is out of bounds, reversed, or not on char boundaries.
    pub fn delete(&mut self, range: Range<usize>) -> Result<(), RangeError> {
        self.replace(range, "")
    }

    /// Replaces the text in byte range `range` with `text`.
    ///
    /// # Errors
    /// [`RangeError`] when `range` is out of bounds, reversed, or not on char boundaries.
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Result<(), RangeError> {
        self.check(&range)?;
        let removed = self.text.get(range.clone()).unwrap_or_default().to_owned();
        if removed.is_empty() && text.is_empty() {
            return Ok(());
        }
        self.text.replace_range(range.clone(), text);
        self.undo_steps.push(Step {
            at: range.start,
            removed,
            inserted: text.to_owned(),
        });
        self.redo_steps.clear();
        Ok(())
    }

    /// Reverts the last step. `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo_steps.pop() else {
            return false;
        };
        let step = self.apply(step);
        self.redo_steps.push(step);
        true
    }

    /// Applies the last undone step again. `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo_steps.pop() else {
            return false;
        };
        let step = self.apply(step);
        self.undo_steps.push(step);
        true
    }

    /// Reverts `step` and returns the step that reverts it back.
    fn apply(&mut self, step: Step) -> Step {
        let end = step.at + step.inserted.len();
        self.text.replace_range(step.at..end, &step.removed);
        step.inverse()
    }

    fn check(&self, range: &Range<usize>) -> Result<(), RangeError> {
        if self.text.get(range.clone()).is_some() {
            Ok(())
        } else {
            Err(RangeError {
                range: range.clone(),
            })
        }
    }
}
