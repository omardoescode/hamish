mod claude;

use std::fmt;
use std::ops::Range;
use std::time::Duration;

pub use claude::ClaudeBackend;

use crate::{
    Document, LabFinding, LabRequest, Level, Provenance, RangeError, SlotError, SlotId,
    TrimRequest, Variant, paragraph_around, sentence_around,
};

/// How the writer asked for AI Variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTrigger {
    /// "AI alternatives" in the context menu, or its key (FR-32).
    ContextMenu,
    /// "??" typed in the empty row of the Alternatives panel (FR-69).
    QuickFill,
}

/// What the AI gets: the selected text, its sentence and paragraph, and the Variants it
/// has already (FR-75, FR-76).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRequest {
    range: Range<usize>,
    level: Level,
    trigger: AiTrigger,
    selection: String,
    sentence: String,
    paragraph: String,
    variants: Vec<String>,
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

    #[must_use]
    pub fn trigger(&self) -> AiTrigger {
        self.trigger
    }

    /// The sentence that holds the selection.
    #[must_use]
    pub fn sentence(&self) -> &str {
        &self.sentence
    }

    /// The paragraph that holds the selection.
    #[must_use]
    pub fn paragraph(&self) -> &str {
        &self.paragraph
    }

    /// The texts the Slot over the selection has already, so the AI does not repeat them.
    /// With no Slot there, the selection.
    #[must_use]
    pub fn variants(&self) -> &[String] {
        &self.variants
    }
}

/// The one way to ask an AI for Variants, Lab checks, and Trims. Implementations block, so the app calls them
/// on a background thread.
pub trait AiAdapter: Send + Sync {
    /// Variant texts for the selection of `request`.
    ///
    /// # Errors
    /// [`AiError`] when the AI gives no usable answer.
    fn variants(&self, request: &AiRequest) -> Result<Vec<String>, AiError>;

    /// The ranges of the request text that match the Lab check of `request`.
    /// It never changes text.
    ///
    /// # Errors
    /// [`AiError`] when the AI gives no usable answer.
    fn lab_check(&self, request: &LabRequest) -> Result<Vec<LabFinding>, AiError>;

    /// The byte ranges of the request text to cut for the Trim level of `request`.
    /// It never rewrites text.
    ///
    /// # Errors
    /// [`AiError`] when the AI gives no usable answer.
    fn trim(&self, request: &TrimRequest) -> Result<Vec<Range<usize>>, AiError>;
}

/// Why an AI call gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    /// The AI program did not start or could not be watched.
    Start(String),
    /// The AI program ended with a failure.
    Exit { code: Option<i32>, message: String },
    /// The AI gave no structured answer, for example it declined.
    Declined(String),
    /// The output was not the expected shape.
    BadOutput(String),
    /// No answer within this time; the call was stopped.
    Timeout(Duration),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(detail) => write!(f, "the AI did not start: {detail}"),
            Self::Exit {
                code: Some(code),
                message,
            } => write!(f, "the AI failed (exit {code}): {message}"),
            Self::Exit {
                code: None,
                message,
            } => write!(f, "the AI was stopped: {message}"),
            Self::Declined(message) => write!(f, "the AI declined: {message}"),
            Self::BadOutput(detail) => write!(f, "the AI gave bad output: {detail}"),
            Self::Timeout(limit) => write!(f, "the AI gave no answer in {} s", limit.as_secs()),
        }
    }
}

impl std::error::Error for AiError {}

/// Why an AI result was not added. A refused result changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    /// The text at the request range is not the requested selection any more.
    Stale,
    /// No text was new: all were blank or repeated a Variant.
    NoVariants,
    Slot(SlotError),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale => f.write_str("the text changed since the AI request"),
            Self::NoVariants => f.write_str("the AI gave no new Variant"),
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
    /// An AI request for the text in byte range `range`, with its sentence, its paragraph,
    /// and the Variants of the innermost Slot over exactly `range` as context.
    ///
    /// # Errors
    /// [`RangeError`] when `range` is out of bounds, reversed, or not on char boundaries.
    pub fn ai_request(
        &self,
        range: Range<usize>,
        level: Level,
        trigger: AiTrigger,
    ) -> Result<AiRequest, RangeError> {
        let text = self.text();
        let selection = text
            .get(range.clone())
            .ok_or(RangeError {
                range: range.clone(),
            })?
            .to_owned();
        let around = |span: Range<usize>| text.get(span).unwrap_or_default().to_owned();
        let variants = self
            .slots()
            .iter()
            .rev()
            .find(|slot| slot.range == range)
            .map_or_else(
                || vec![selection.clone()],
                |slot| {
                    slot.variants
                        .iter()
                        .map(|variant| variant.text().to_owned())
                        .collect()
                },
            );
        Ok(AiRequest {
            sentence: around(sentence_around(text, &range)),
            paragraph: around(paragraph_around(text, &range)),
            range,
            level,
            trigger,
            selection,
            variants,
        })
    }

    /// Adds `texts` as AI Variants to the Slot over the range of `request`, or to a new Slot
    /// there, as one undoable step. Of Slots with the same range, the innermost gets them.
    /// The Active variant stays (FR-32, FR-36).
    ///
    /// # Errors
    /// [`ApplyError`] when the result cannot be added.
    pub fn apply_ai_result(
        &mut self,
        request: &AiRequest,
        texts: &[String],
    ) -> Result<SlotId, ApplyError> {
        if self.text().get(request.range()) != Some(request.selection()) {
            return Err(ApplyError::Stale);
        }
        let slots = self.slots();
        let existing = slots.iter().rev().find(|slot| slot.range == request.range);
        let mut known: Vec<&str> = existing.map_or_else(
            || vec![request.selection()],
            |slot| slot.variants.iter().map(Variant::text).collect(),
        );
        let mut new = Vec::new();
        for text in texts {
            if !text.trim().is_empty() && !known.contains(&text.as_str()) {
                known.push(text);
                new.push(text.as_str());
            }
        }
        if new.is_empty() {
            return Err(ApplyError::NoVariants);
        }
        let texts = new;
        let existing = existing.map(|slot| slot.id);
        let Some(id) = existing else {
            return Ok(self.create_slot_with(
                request.range(),
                request.level,
                &texts,
                Provenance::Ai,
            )?);
        };
        self.add_variants(id, &texts, Provenance::Ai)?;
        Ok(id)
    }
}
