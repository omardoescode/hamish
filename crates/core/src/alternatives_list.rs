//! The Alternatives list: the target, rows, draft, edit and Quick fill reveal of the
//! Alternatives panel, and how each writer input changes them. No GPUI (ADR 0005).

use std::fmt;
use std::ops::Range;

use crate::{
    AiRequest, AiTrigger, ApplyError, Document, EditSession, Level, Provenance, RangeError,
    SlotError, SlotId, Switch, follow_change, paragraph_around, sentence_around, word_at,
};

/// The draft that starts a Quick fill. It never becomes a Variant.
pub const QUICK_FILL: &str = "??";

/// Quick fill results show this many at a time.
const QUICK_FILL_BATCH: usize = 2;

/// What the Alternatives list changes the Document through. Each change is one undo step.
pub trait ListEditor {
    fn document(&self) -> &Document;

    /// Runs `change` on the Document as one undo step.
    ///
    /// # Errors
    /// The error of `change`.
    fn change_document<T, E>(
        &mut self,
        change: impl FnOnce(&mut Document) -> Result<T, E>,
    ) -> Result<T, E>;

    /// Deletes Variant `index` of Slot `id` as one undo step. Returns the range of the
    /// kept text when the Slot ends.
    ///
    /// # Errors
    /// The error of [`Document::delete_variant`].
    fn delete_variant(
        &mut self,
        id: SlotId,
        index: usize,
    ) -> Result<Option<Range<usize>>, SlotError> {
        self.change_document(|document| document.delete_variant(id, index))
    }
}

impl ListEditor for EditSession {
    fn document(&self) -> &Document {
        EditSession::document(self)
    }

    fn change_document<T, E>(
        &mut self,
        change: impl FnOnce(&mut Document) -> Result<T, E>,
    ) -> Result<T, E> {
        self.change(change).map(|(done, _)| done)
    }
}

/// One Variant row of the Alternatives list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRow {
    pub text: String,
    pub provenance: Provenance,
    /// The Active variant.
    pub active: bool,
}

/// Why the list asked the AI nothing, or core refused its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiAlternativesError {
    /// The list shows no Slot and no text.
    NoTarget,
    Range(RangeError),
    Apply(ApplyError),
}

impl fmt::Display for AiAlternativesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTarget => f.write_str("select text first"),
            Self::Range(error) => error.fmt(f),
            Self::Apply(error) => error.fmt(f),
        }
    }
}

/// What the list shows: a Slot, or text with no Slot yet (its text is the Original).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListTarget {
    Slot(SlotId),
    Text { range: Range<usize>, level: Level },
    Nothing,
}

impl ListTarget {
    /// The innermost Slot that holds `range`, else the text of `range`.
    fn at(document: &Document, range: Range<usize>) -> Self {
        if let Some(id) = document.slot_over(&range) {
            return Self::Slot(id);
        }
        if range.is_empty() {
            return Self::Nothing;
        }
        let level = document.level_for(&range);
        Self::Text { range, level }
    }

    /// The range and Level the target covers now. `None` with nothing to show.
    fn place(&self, document: &Document) -> Option<(Range<usize>, Level)> {
        match self {
            Self::Slot(id) => document.slot(*id).map(|slot| (slot.range, slot.level)),
            Self::Text { range, level } => Some((range.clone(), *level)),
            Self::Nothing => None,
        }
    }
}

/// What the panel does after a writer input. The list changed the Document already.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ListOutcome {
    /// The Active variant changed: its Cycle sound plays.
    pub switch: Option<Switch>,
    /// The draft or the target changed: the preview shows again.
    pub preview: bool,
    /// The draft became "??": the panel asks the AI for a Quick fill.
    pub quick_fill: bool,
    /// The selected row changed: the list scrolls to it.
    pub scroll: bool,
}

impl ListOutcome {
    const PREVIEW: Self = Self {
        switch: None,
        preview: true,
        quick_fill: false,
        scroll: false,
    };
}

/// The state of the Alternatives list. Each input reads the Document, or changes it
/// through a [`ListEditor`], and returns what the panel does next.
#[derive(Debug, Clone)]
pub struct AlternativesList {
    target: ListTarget,
    /// The selected row: a Variant, or `rows().len()` for the last row, which takes a new one.
    row: usize,
    /// The text typed in the last row.
    draft: String,
    /// The text of the Variant in the selected row while the writer edits it.
    editing: Option<String>,
    /// Quick fill results at the end of the list not shown yet. `None` with no reveal.
    hidden: Option<usize>,
    /// The last row is hidden after a Quick fill, until the writer goes to it.
    new_row_hidden: bool,
    /// The Document text at the last change, to move a text target along with edits.
    seen_text: String,
}

impl AlternativesList {
    /// The list of the Slot that holds `at` (innermost first), else of the text of `at`,
    /// with the last row selected.
    #[must_use]
    pub fn new(document: &Document, at: Range<usize>) -> Self {
        let mut list = Self {
            target: ListTarget::at(document, at),
            row: 0,
            draft: String::new(),
            editing: None,
            hidden: None,
            new_row_hidden: false,
            seen_text: document.text().to_owned(),
        };
        list.row = list.rows(document).len();
        list
    }

    #[must_use]
    pub fn target(&self) -> &ListTarget {
        &self.target
    }

    /// The selected row; `rows().len()` is the last row.
    #[must_use]
    pub fn row(&self) -> usize {
        self.row
    }

    /// The text typed in the last row.
    #[must_use]
    pub fn draft(&self) -> &str {
        &self.draft
    }

    /// The text of the Variant in the selected row while the writer edits it.
    #[must_use]
    pub fn editing(&self) -> Option<&str> {
        self.editing.as_deref()
    }

    /// The last row is hidden after a Quick fill, until the writer goes to it.
    #[must_use]
    pub fn new_row_hidden(&self) -> bool {
        self.new_row_hidden
    }

    /// Quick fill results still come in.
    #[must_use]
    pub fn is_revealing(&self) -> bool {
        self.hidden.is_some()
    }

    /// The Level of the target. `None` with nothing to show.
    #[must_use]
    pub fn level(&self, document: &Document) -> Option<Level> {
        self.target.place(document).map(|(_, level)| level)
    }

    /// The Variants in order, the Original first. Quick fill results not shown yet are
    /// left out.
    #[must_use]
    pub fn rows(&self, document: &Document) -> Vec<VariantRow> {
        let mut rows = self.all_rows(document);
        rows.truncate(rows.len().saturating_sub(self.hidden.unwrap_or(0)));
        rows
    }

    fn all_rows(&self, document: &Document) -> Vec<VariantRow> {
        match &self.target {
            ListTarget::Slot(id) => document.slot(*id).map_or_default(|slot| {
                slot.variants
                    .iter()
                    .enumerate()
                    .map(|(index, variant)| VariantRow {
                        text: variant.text().to_owned(),
                        provenance: variant.provenance(),
                        active: index == slot.active,
                    })
                    .collect()
            }),
            ListTarget::Text { range, .. } => document
                .text()
                .get(range.clone())
                .map(|text| VariantRow {
                    text: text.to_owned(),
                    provenance: Provenance::Writer,
                    active: true,
                })
                .into_iter()
                .collect(),
            ListTarget::Nothing => Vec::new(),
        }
    }

    /// The draft shows in the Document in place of this range while it has text that does
    /// not start a Quick fill.
    #[must_use]
    pub fn preview(&self, document: &Document) -> Option<(Range<usize>, &str)> {
        let (range, _) = self.target.place(document)?;
        (!self.draft.is_empty() && self.draft != QUICK_FILL).then_some((range, self.draft.as_str()))
    }

    fn set_target(&mut self, target: ListTarget, document: &Document) {
        self.target = target;
        document.text().clone_into(&mut self.seen_text);
    }

    /// A text target moves along with a Document edit. Text edited away leaves no target.
    /// `true` when the target moved.
    pub fn follow_document(&mut self, document: &Document) -> bool {
        let text = document.text();
        if text == self.seen_text {
            return false;
        }
        let old = std::mem::replace(&mut self.seen_text, text.to_owned());
        let ListTarget::Text { range, level } = &self.target else {
            return false;
        };
        let range = follow_change(range.start, &old, text)..follow_change(range.end, &old, text);
        let target = if range.is_empty() {
            ListTarget::Nothing
        } else {
            ListTarget::Text {
                range,
                level: *level,
            }
        };
        self.set_target(target, document);
        true
    }

    /// Shows Slot `id`, with the last row selected and no draft.
    pub fn show_slot(&mut self, document: &Document, id: SlotId) -> ListOutcome {
        self.set_target(ListTarget::Slot(id), document);
        self.editing = None;
        self.draft.clear();
        self.row = self.rows(document).len();
        ListOutcome::PREVIEW
    }

    /// Shows the word, sentence, or paragraph around the target, or the Slot of that range
    /// and Level when there is one.
    pub fn choose_level(&mut self, document: &Document, level: Level) -> ListOutcome {
        let Some((anchor, _)) = self.target.place(document) else {
            return ListOutcome::default();
        };
        let text = document.text();
        let range = match level {
            Level::Word => word_at(text, anchor.start).unwrap_or(anchor),
            Level::Sentence => sentence_around(text, &anchor),
            Level::Paragraph => paragraph_around(text, &anchor),
        };
        let slot = document
            .slots()
            .into_iter()
            .rev()
            .find(|slot| slot.range == range && slot.level == level)
            .map(|slot| slot.id);
        let target = slot.map_or(ListTarget::Text { range, level }, ListTarget::Slot);
        self.set_target(target, document);
        self.editing = None;
        self.row = self.rows(document).len();
        ListOutcome::PREVIEW
    }

    /// The AI request for the Slot or text the list shows.
    ///
    /// # Errors
    /// [`AiAlternativesError::NoTarget`] with nothing to show,
    /// [`AiAlternativesError::Range`] for a target range core refuses.
    pub fn ai_request(
        &self,
        document: &Document,
        trigger: AiTrigger,
    ) -> Result<AiRequest, AiAlternativesError> {
        let (range, level) = self
            .target
            .place(document)
            .ok_or(AiAlternativesError::NoTarget)?;
        document
            .ai_request(range, level, trigger)
            .map_err(AiAlternativesError::Range)
    }

    /// Adds the AI texts as AI Variants in one undo step; the Active variant stays. A Quick
    /// fill then reveals them in batches (see [`Self::reveal_batch`]); a context-menu request
    /// shows the Slot that got them.
    ///
    /// # Errors
    /// The [`ApplyError`] of core.
    pub fn answered(
        &mut self,
        editor: &mut impl ListEditor,
        request: &AiRequest,
        texts: &[String],
    ) -> Result<(), ApplyError> {
        let at_last_row = self.row >= self.rows(editor.document()).len();
        let before = self.all_rows(editor.document()).len();
        let id = editor.change_document(|document| document.apply_ai_result(request, texts))?;
        let document = editor.document();
        match request.trigger() {
            AiTrigger::QuickFill => self.reveal(document, id, before),
            AiTrigger::ContextMenu => self.show_answer(document, id, request, at_last_row),
        }
        Ok(())
    }

    /// Hides the Variants a Quick fill added to Slot `id`; the list had `before` rows.
    fn reveal(&mut self, document: &Document, id: SlotId, before: usize) {
        if matches!(self.target, ListTarget::Text { .. }) {
            self.set_target(ListTarget::Slot(id), document);
        }
        let hidden = self.all_rows(document).len().saturating_sub(before.max(1));
        self.hidden = Some(hidden);
    }

    /// The list shows Slot `id` when it showed the text of `request`.
    fn show_answer(
        &mut self,
        document: &Document,
        id: SlotId,
        request: &AiRequest,
        at_last_row: bool,
    ) {
        let showed_request = match &self.target {
            ListTarget::Text { range, .. } => *range == request.range(),
            ListTarget::Slot(shown) => *shown == id,
            ListTarget::Nothing => false,
        };
        if showed_request {
            self.set_target(ListTarget::Slot(id), document);
        }
        if at_last_row {
            self.row = self.rows(document).len();
        }
    }

    /// Shows the next batch of Quick fill results. `false` when none are left: the Quick
    /// fill is done, the Active variant row is selected and the last row hides.
    pub fn reveal_batch(&mut self, document: &Document) -> bool {
        let Some(hidden) = self.hidden.as_mut() else {
            return false;
        };
        *hidden = hidden.saturating_sub(QUICK_FILL_BATCH);
        if *hidden > 0 {
            return true;
        }
        self.hidden = None;
        self.new_row_hidden = true;
        let rows = self.rows(document);
        self.row = rows.iter().position(|row| row.active).unwrap_or(rows.len());
        false
    }

    /// Stops a Quick fill: results already added stay, and the draft is empty.
    pub fn cancel_quick_fill(&mut self) {
        self.hidden = None;
        self.draft.clear();
    }

    /// Return saves the edit in progress, starts an edit on a Variant row, or adds the
    /// draft of the last row as the Active variant. With no Slot yet, the add makes one
    /// over the target text, the Original first.
    pub fn confirm(&mut self, editor: &mut impl ListEditor) -> ListOutcome {
        if let Some(text) = self.editing.take() {
            self.save_edit(editor, &text);
            return ListOutcome::default();
        }
        let rows = self.rows(editor.document());
        let Some(row) = rows.get(self.row) else {
            return self.add(editor);
        };
        if matches!(self.target, ListTarget::Slot(_)) {
            self.editing = Some(row.text.clone());
        }
        ListOutcome::default()
    }

    /// Sets the text of the Variant in the selected row, as a writer edit.
    fn save_edit(&self, editor: &mut impl ListEditor, text: &str) {
        let ListTarget::Slot(id) = self.target else {
            return;
        };
        let row = self.row;
        editor
            .change_document(|document| document.edit_variant(id, row, text))
            .ok();
    }

    fn add(&mut self, editor: &mut impl ListEditor) -> ListOutcome {
        if self.draft.is_empty() || self.draft == QUICK_FILL {
            return ListOutcome::default();
        }
        let draft = self.draft.clone();
        let switched = match self.target.clone() {
            ListTarget::Slot(id) => editor
                .change_document(|document| {
                    document.add_active_variant(id, &draft, Provenance::Writer)
                })
                .ok(),
            ListTarget::Text { range, level } => editor
                .change_document(|document| {
                    document.create_active_slot(range, level, &draft, Provenance::Writer)
                })
                .ok()
                .map(|id| {
                    self.set_target(ListTarget::Slot(id), editor.document());
                    Switch::ToVariant
                }),
            ListTarget::Nothing => None,
        };
        let Some(switch) = switched else {
            return ListOutcome::default();
        };
        self.draft.clear();
        self.row = self.rows(editor.document()).len();
        ListOutcome {
            switch: Some(switch),
            ..ListOutcome::PREVIEW
        }
    }

    /// Selects row `row` (past the end: the last row). A Variant row shows that Variant in
    /// the Document at once, as one undo step; the last row changes nothing.
    pub fn select(&mut self, editor: &mut impl ListEditor, row: usize) -> ListOutcome {
        let last = self.rows(editor.document()).len();
        let row = row.min(last);
        if row == last {
            self.new_row_hidden = false;
        }
        self.row = row;
        self.editing = None;
        let mut outcome = ListOutcome {
            scroll: true,
            ..ListOutcome::default()
        };
        let ListTarget::Slot(id) = self.target else {
            return outcome;
        };
        let active = editor.document().slot(id).map(|slot| slot.active);
        if active.is_none_or(|active| active == row) {
            return outcome;
        }
        outcome.switch = editor
            .change_document(|document| document.set_active(id, row))
            .ok();
        outcome
    }

    /// Esc drops the edit in progress. `true` when there was none: the panel closes.
    pub fn escape(&mut self) -> bool {
        self.editing.take().is_none()
    }

    /// Backspace deletes the last char of the edit in progress, else of the draft.
    pub fn delete_backward(&mut self) -> ListOutcome {
        if let Some(text) = self.editing.as_mut() {
            text.pop();
            return ListOutcome::default();
        }
        self.draft.pop();
        ListOutcome::PREVIEW
    }

    /// Deletes the Variant in the selected row (the Original too). A Slot left with one
    /// Variant ends; the list then shows its text with no Slot. Returns the span of the
    /// deleted Variant, `None` when nothing was deleted.
    pub fn delete_variant(&mut self, editor: &mut impl ListEditor) -> Option<Range<usize>> {
        let ListTarget::Slot(id) = self.target else {
            return None;
        };
        let slot = editor.document().slot(id)?;
        let row = self.row;
        if row >= slot.variants.len() {
            return None;
        }
        let (level, range) = (slot.level, slot.range.clone());
        let kept = editor.delete_variant(id, row).ok()?;
        if let Some(kept) = kept {
            self.set_target(ListTarget::Text { range: kept, level }, editor.document());
        }
        self.editing = None;
        self.row = self.rows(editor.document()).len();
        Some(range)
    }

    /// After an undo or redo the edit in progress ends. A Slot that the step removed gives
    /// way to the target at `at`.
    pub fn after_undo_step(&mut self, document: &Document, at: Range<usize>) {
        self.editing = None;
        if let ListTarget::Slot(id) = self.target
            && document.slot(id).is_none()
        {
            self.set_target(ListTarget::at(document, at), document);
        }
        self.row = self.row.min(self.rows(document).len());
    }

    /// Typed text goes into the edit in progress, else into the last row, which it selects.
    /// A line break types a space. "??" in the last row starts a Quick fill.
    pub fn type_text(&mut self, document: &Document, typed: &str) -> ListOutcome {
        let typed = typed.replace('\n', " ");
        if let Some(text) = self.editing.as_mut() {
            text.push_str(&typed);
            return ListOutcome::default();
        }
        self.row = self.rows(document).len();
        self.new_row_hidden = false;
        self.draft.push_str(&typed);
        let quick_fill = self.draft == QUICK_FILL;
        if quick_fill {
            self.draft.clear();
        }
        ListOutcome {
            quick_fill,
            scroll: true,
            ..ListOutcome::PREVIEW
        }
    }
}
