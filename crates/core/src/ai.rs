use std::fmt;
use std::ops::Range;

use crate::{Document, Level, Provenance, RangeError, SlotError, SlotId};

/// What the AI gets: the selected text and the text around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRequest {
    range: Range<usize>,
    level: Level,
    selection: String,
    context: String,
}

impl AiRequest {
    /// Byte range of the selection in the active text when the request was made.
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    #[must_use]
    pub fn level(&self) -> Level {
        self.level
    }

    #[must_use]
    pub fn selection(&self) -> &str {
        &self.selection
    }

    /// The active text of the whole Document.
    #[must_use]
    pub fn context(&self) -> &str {
        &self.context
    }
}

/// The one way to ask an AI for Variants. Implementations block, so the app calls them
/// on a background thread.
pub trait AiAdapter: Send + Sync {
    /// Variant texts for the selection of `request`.
    ///
    /// # Errors
    /// [`AiError`] when the AI gives no usable answer.
    fn variants(&self, request: &AiRequest) -> Result<Vec<String>, AiError>;
}

/// Why an AI call gave no Variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    /// The output was not the expected shape.
    BadOutput(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadOutput(detail) => write!(f, "the AI gave bad output: {detail}"),
        }
    }
}

impl std::error::Error for AiError {}

/// Why an AI result was not added. A refused result changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    Slot(SlotError),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Slot(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ApplyError {}

impl From<SlotError> for ApplyError {
    fn from(error: SlotError) -> Self {
        Self::Slot(error)
    }
}

impl Document {
    /// An AI request for the text in byte range `range`, with the whole active text as context.
    ///
    /// # Errors
    /// [`RangeError`] when `range` is out of bounds, reversed, or not on char boundaries.
    pub fn ai_request(&self, range: Range<usize>, level: Level) -> Result<AiRequest, RangeError> {
        let selection = self
            .text()
            .get(range.clone())
            .ok_or(RangeError {
                range: range.clone(),
            })?
            .to_owned();
        Ok(AiRequest {
            range,
            level,
            selection,
            context: self.text().to_owned(),
        })
    }

    /// Adds `texts` as AI Variants to the Slot of `request`, as one undoable step.
    /// The Active variant stays (FR-32, FR-36).
    ///
    /// # Errors
    /// [`ApplyError`] when the result cannot be added.
    pub fn apply_ai_result(
        &mut self,
        request: &AiRequest,
        texts: &[String],
    ) -> Result<SlotId, ApplyError> {
        let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
        Ok(self.create_slot_with(request.range(), request.level, &texts, Provenance::Ai)?)
    }
}
