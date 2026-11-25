//! The Document: its text, Slots, Ghost ranges, Overflow, Lab marks, and Trim proposal,
//! with core undo and redo.

use std::fmt;
use std::ops::Range;

use crate::article;
use crate::ghost;
use crate::markdown;
use crate::slot::{self, SlotNode};
use crate::{
    Counter, Cycle, GhostError, LabMark, LabMarkId, Level, MarkdownSpan, Overflow, OverflowError,
    Provenance, Slot, SlotError, SlotId, Switch, TrimProposal, Variant, paragraph_around,
};

/// One piece of writing: its text, Slots, Ghost ranges, Overflow, Lab marks, Trim proposal,
/// and edit history.
#[derive(Debug, Clone, Default)]
pub struct Document {
    text: String,
    slots: Vec<SlotNode>,
    ghosts: Vec<Range<usize>>,
    overflow: Overflow,
    next_slot: SlotId,
    lab_marks: Vec<LabMark>,
    next_lab_mark: LabMarkId,
    pub(crate) trim: Option<TrimProposal>,
    undo_steps: Vec<Step>,
    redo_steps: Vec<Step>,
}

/// How a Slot change reaches the text. Only a writer text edit makes an
/// outer AI Variant Mixed; a Variant switch does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotChange {
    Switch,
    WriterText,
}

/// One undoable change: `removed` at `at` was swapped for `inserted`,
/// the Slots, Ghost ranges, and Overflow went from `before` to `after`, the Lab marks likewise.
#[derive(Debug, Clone)]
struct Step {
    at: usize,
    removed: String,
    inserted: String,
    before: Tree,
    after: Tree,
    marks_before: Vec<LabMark>,
    marks_after: Vec<LabMark>,
    /// The step deleted a Variant of this Slot (its undo plays a burst).
    variant_deleted: Option<SlotId>,
}

/// The Slots, top-level Ghost ranges (ADR 0006), and Overflow of the Document at one time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Tree {
    slots: Vec<SlotNode>,
    ghosts: Vec<Range<usize>>,
    overflow: Overflow,
}

impl Step {
    fn inverse(self) -> Self {
        Self {
            at: self.at,
            removed: self.inserted,
            inserted: self.removed,
            before: self.after,
            after: self.before,
            marks_before: self.marks_after,
            marks_after: self.marks_before,
            variant_deleted: self.variant_deleted,
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
    /// A Document with `text`, and no Slots, Ghost ranges, Overflow, or history.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            ..Self::default()
        }
    }

    /// The active text: each Slot shows its Active variant.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Counter of the Document. Ghost text is not counted; a Ghost range splits words.
    #[must_use]
    pub fn counter(&self) -> Counter {
        self.count_outside(0..self.text.len(), &[])
    }

    /// The visible Ghost ranges in document order, as byte ranges of the active text.
    #[must_use]
    pub fn ghost_ranges(&self) -> Vec<Range<usize>> {
        let mut nested = Vec::new();
        slot::collect_ghosts(&self.slots, 0, &mut nested);
        let mut out = self.ghosts.clone();
        for range in nested {
            ghost::join(&mut out, range);
        }
        out
    }

    /// Ghosts `range`: the text stays in the Document, dimmed. Ghost ranges
    /// that overlap or touch it join it. A range inside a Slot is Ghost only in that Slot's
    /// Active variant (ADR 0006); a range that holds a Slot stays Ghost whichever Variant shows.
    ///
    /// # Errors
    /// [`GhostError::Range`] for an invalid range, [`GhostError::EmptyRange`] for an empty one,
    /// [`GhostError::PartialOverlap`] when `range` partly overlaps a Slot.
    pub fn ghost(&mut self, range: Range<usize>) -> Result<(), GhostError> {
        self.text.get(range.clone()).ok_or(RangeError {
            range: range.clone(),
        })?;
        if range.is_empty() {
            return Err(GhostError::EmptyRange);
        }
        let mut slots = self.slots.clone();
        let mut ghosts = self.ghosts.clone();
        slot::place_ghost(&mut slots, &mut ghosts, range.clone())?;
        self.commit(range.start..range.start, "", self.tree(slots, ghosts))?;
        Ok(())
    }

    /// Revives each visible Ghost range that `range` touches. An empty `range`
    /// (a caret) touches a Ghost range it lies in or on an edge of.
    ///
    /// # Errors
    /// [`GhostError::NoGhost`] when `range` touches no Ghost range.
    pub fn revive(&mut self, range: Range<usize>) -> Result<(), GhostError> {
        let mut slots = self.slots.clone();
        let mut ghosts = self.ghosts.clone();
        if slot::revive(&mut slots, &mut ghosts, &range) == 0 {
            return Err(GhostError::NoGhost(range));
        }
        self.commit(range.start..range.start, "", self.tree(slots, ghosts))?;
        Ok(())
    }

    /// Counter of the text in `range` outside Ghost ranges and `excluded`, each piece on its own.
    pub(crate) fn count_outside(&self, range: Range<usize>, excluded: &[Range<usize>]) -> Counter {
        let mut skipped = self.ghost_ranges();
        skipped.extend_from_slice(excluded);
        skipped.sort_by_key(|skip| skip.start);
        let mut pieces = Vec::new();
        let mut from = range.start;
        for ghost in skipped {
            let to = ghost.start.clamp(from, range.end);
            pieces.push(from..to);
            from = ghost.end.clamp(from, range.end);
        }
        pieces.push(from..range.end);
        pieces
            .into_iter()
            .filter_map(|piece| self.text.get(piece))
            .map(Counter::of)
            .fold(Counter::default(), |all, piece| Counter {
                words: all.words + piece.words,
                chars: all.chars + piece.chars,
            })
    }

    /// The Markdown styles of the active text, ordered by start; outer before inner.
    #[must_use]
    pub fn markdown_styles(&self) -> Vec<MarkdownSpan> {
        markdown::spans(&self.text)
    }

    /// The first line, shown as the title. `None` when the first line is blank.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.text
            .lines()
            .next()
            .filter(|line| !line.trim().is_empty())
    }

    /// Counter of the text in byte range `selection`. Ghost text is not counted.
    ///
    /// # Errors
    /// [`RangeError`] when `selection` is out of bounds, reversed, or not on char boundaries.
    pub fn selection_counter(&self, selection: Range<usize>) -> Result<Counter, RangeError> {
        if self.text.get(selection.clone()).is_none() {
            return Err(RangeError { range: selection });
        }
        Ok(self.count_outside(selection, &[]))
    }

    /// The visible Slots in document order, each parent before the Slots nested in it.
    #[must_use]
    pub fn slots(&self) -> Vec<Slot<'_>> {
        let mut out = Vec::new();
        slot::collect(&self.slots, 0, &mut out);
        out
    }

    /// The visible Slot `id`. `None` when it ended or sits in an inactive Variant.
    #[must_use]
    pub fn slot(&self, id: SlotId) -> Option<Slot<'_>> {
        self.slots().into_iter().find(|slot| slot.id == id)
    }

    /// The innermost visible Slot whose span holds byte offset `caret`, edges included
    /// (the caret at the end of a span is in it).
    #[must_use]
    pub fn slot_at(&self, caret: usize) -> Option<SlotId> {
        self.slots()
            .iter()
            .rev()
            .find(|slot| slot.range.start <= caret && caret <= slot.range.end)
            .map(|slot| slot.id)
    }

    /// The Level a new Slot over `range` gets: Paragraph for a whole paragraph or more,
    /// Sentence for text with sentence-end punctuation, else Word (a word or short phrase).
    #[must_use]
    pub fn level_for(&self, range: &Range<usize>) -> Level {
        let selected = self.text.get(range.clone()).unwrap_or_default();
        let paragraph = self
            .text
            .get(paragraph_around(&self.text, range))
            .unwrap_or_default()
            .trim();
        if selected.contains('\n') || (!paragraph.is_empty() && selected.trim() == paragraph) {
            return Level::Paragraph;
        }
        if selected.contains(['.', '!', '?']) {
            return Level::Sentence;
        }
        Level::Word
    }

    /// Makes a Slot over `range`: its text becomes the Original, `text` the second Variant.
    /// The Original stays the Active variant. A range inside a visible Slot makes a Slot
    /// nested in that Slot's Active variant (ADR 0006): it shows only while that Variant does.
    ///
    /// # Errors
    /// [`SlotError::Range`] for an invalid range, [`SlotError::EmptyRange`] for an empty one,
    /// [`SlotError::PartialOverlap`] when `range` partly overlaps another Slot.
    pub fn create_slot(
        &mut self,
        range: Range<usize>,
        level: Level,
        text: &str,
        provenance: Provenance,
    ) -> Result<SlotId, SlotError> {
        self.create_slot_with(range, level, &[text], provenance)
    }

    /// Makes a Slot over `range`: its text becomes the Original, `texts` the next Variants,
    /// as one undoable step. The Original stays the Active variant.
    pub(crate) fn create_slot_with(
        &mut self,
        range: Range<usize>,
        level: Level,
        texts: &[&str],
        provenance: Provenance,
    ) -> Result<SlotId, SlotError> {
        let original = self.text.get(range.clone()).ok_or(RangeError {
            range: range.clone(),
        })?;
        if range.is_empty() {
            return Err(SlotError::EmptyRange);
        }
        let id = self.next_slot;
        let variants = std::iter::once(Variant::new(original, Provenance::Writer))
            .chain(texts.iter().map(|text| Variant::new(text, provenance)))
            .collect();
        let node = SlotNode {
            id,
            level,
            range: range.clone(),
            variants,
            active: 0,
        };
        let mut slots = self.slots.clone();
        let mut ghosts = self.ghosts.clone();
        slot::place(&mut slots, &mut ghosts, node)?;
        self.commit(range.start..range.start, "", self.tree(slots, ghosts))?;
        self.next_slot = id.next();
        Ok(id)
    }

    /// Adds a Variant at the end of Slot `id`. The Active variant stays.
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] when `id` is not a visible Slot.
    pub fn add_variant(
        &mut self,
        id: SlotId,
        text: &str,
        provenance: Provenance,
    ) -> Result<(), SlotError> {
        self.change_slot(id, SlotChange::Switch, |node| {
            node.push_variant(text, provenance);
            Ok(())
        })?;
        Ok(())
    }

    /// Adds `texts` as Variants at the end of Slot `id`, as one undoable step.
    pub(crate) fn add_variants(
        &mut self,
        id: SlotId,
        texts: &[&str],
        provenance: Provenance,
    ) -> Result<(), SlotError> {
        self.change_slot(id, SlotChange::Switch, |node| {
            for text in texts {
                node.push_variant(text, provenance);
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Makes Variant `index` of Slot `id` the Active variant.
    /// For a Word Slot, an "a"/"an" just before it changes to agree with the
    /// new text, in the same undoable step (Article agreement).
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] or [`SlotError::NoVariant`] when they do not exist.
    pub fn set_active(&mut self, id: SlotId, index: usize) -> Result<Switch, SlotError> {
        self.change_slot(id, SlotChange::Switch, |node| node.set_active(index))?;
        Ok(Switch::to(index))
    }

    /// Moves the Active variant of Slot `id` one step.
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] when `id` is not a visible Slot.
    pub fn cycle_active(&mut self, id: SlotId, cycle: Cycle) -> Result<Switch, SlotError> {
        let slot = self.slot(id).ok_or(SlotError::NoSlot(id))?;
        let count = slot.variants.len();
        let step = match cycle {
            Cycle::Next => 1,
            Cycle::Previous => count.saturating_sub(1),
        };
        let index = (slot.active + step).checked_rem(count).unwrap_or(0);
        self.set_active(id, index)
    }

    /// Deletes Variant `index` of Slot `id`; any Variant can go, the Original too. When it
    /// was active, the next Variant shows; after the last one, the first.
    /// A Slot left with fewer than two Variants ends; its active text stays. Returns the
    /// byte range of that kept text when the Slot ends (Article agreement may move it), else `None`.
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] or [`SlotError::NoVariant`] when they do not exist.
    pub fn delete_variant(
        &mut self,
        id: SlotId,
        index: usize,
    ) -> Result<Option<Range<usize>>, SlotError> {
        let steps = self.undo_steps.len();
        let kept = self.change_slot(id, SlotChange::Switch, |node| node.remove_variant(index))?;
        if let Some(step) = self.undo_steps.get_mut(steps) {
            step.variant_deleted = Some(id);
        }
        Ok(kept)
    }

    /// The Slot whose Variant the next [`Document::undo`] puts back: `Some` when the last
    /// step is a Variant delete.
    #[must_use]
    pub fn undo_restores_variant(&self) -> Option<SlotId> {
        self.undo_steps.last()?.variant_deleted
    }

    /// Sets the text of Variant `index` of Slot `id`, as a writer edit:
    /// an AI Variant becomes Mixed. Slots nested in that Variant end.
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] or [`SlotError::NoVariant`] when they do not exist.
    pub fn edit_variant(&mut self, id: SlotId, index: usize, text: &str) -> Result<(), SlotError> {
        self.change_slot(id, SlotChange::WriterText, |node| {
            node.edit_variant(index, text)
        })?;
        Ok(())
    }

    /// Runs `change` on Slot `id`, then shows its Active variant text in its range,
    /// as one undoable step. Returns the range of the shown text when the Slot ends.
    fn change_slot(
        &mut self,
        id: SlotId,
        kind: SlotChange,
        change: impl FnOnce(&mut SlotNode) -> Result<(), SlotError>,
    ) -> Result<Option<Range<usize>>, SlotError> {
        let mut slots = self.slots.clone();
        let (node, base) = slot::find_mut(&mut slots, id, 0).ok_or(SlotError::NoSlot(id))?;
        let old = base + node.range.start..base + node.range.end;
        change(node)?;
        let shown = node.active_text()?.to_owned();
        node.range.end = node.range.start + shown.len();
        let shown_changed = self.text.get(old.clone()) != Some(shown.as_str());
        let writer_edit = kind == SlotChange::WriterText && shown_changed;
        let agrees = node.level == Level::Word && shown_changed;
        let ends = node.variants.len() < 2;
        let mut ghosts = self.ghosts.clone();
        slot::shift(
            &mut slots,
            &mut ghosts,
            &old,
            shown.len(),
            writer_edit,
            Some(id),
        );
        if ends {
            slot::dissolve(&mut slots, &mut ghosts, id);
        }
        let Some((article_range, article)) = agrees
            .then(|| self.article_agreement(id, &old, &shown))
            .flatten()
        else {
            let kept = ends.then(|| old.start..old.start + shown.len());
            self.commit(old, &shown, self.tree(slots, ghosts))?;
            return Ok(kept);
        };
        slot::shift(
            &mut slots,
            &mut ghosts,
            &article_range,
            article.len(),
            false,
            None,
        );
        let gap = self
            .text
            .get(article_range.end..old.start)
            .unwrap_or_default();
        let inserted = format!("{article}{gap}{shown}");
        let start = old.start - article_range.len() + article.len();
        let kept = ends.then(|| start..start + shown.len());
        self.commit(
            article_range.start..old.end,
            &inserted,
            self.tree(slots, ghosts),
        )?;
        Ok(kept)
    }

    /// Article agreement for Word Slot `id` at `old`, now showing `shown`:
    /// the range of the "a"/"an" before it and the article that agrees.
    /// `None` when there is no such article, it already agrees, or it is part of
    /// a Slot that does not hold the Word Slot.
    fn article_agreement(
        &self,
        id: SlotId,
        old: &Range<usize>,
        shown: &str,
    ) -> Option<(Range<usize>, &'static str)> {
        let before = self.text.get(..old.start)?;
        let gap_start = before.trim_end().len();
        if gap_start == old.start {
            return None;
        }
        let head = before.get(..gap_start)?;
        let start = head.trim_end_matches(char::is_alphanumeric).len();
        let current = head.get(start..)?;
        let wanted = article::agreeing(current, shown)?;
        let reach = start..old.end;
        let blocked = self.slots().iter().any(|slot| {
            slot.id != id
                && !(slot.range.end <= start || old.start <= slot.range.start)
                && !(slot.range.start <= reach.start && reach.end <= slot.range.end)
        });
        (wanted != current && !blocked).then_some((start..gap_start, wanted))
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
        let removed = self.text.get(range.clone()).ok_or(RangeError {
            range: range.clone(),
        })?;
        if removed.is_empty() && text.is_empty() {
            return Ok(());
        }
        let writer_edit = removed != text;
        let mut slots = self.slots.clone();
        let mut ghosts = self.ghosts.clone();
        slot::shift(
            &mut slots,
            &mut ghosts,
            &range,
            text.len(),
            writer_edit,
            None,
        );
        self.commit(range, text, self.tree(slots, ghosts))
    }

    /// Deletes `ranges` as one undoable writer edit. `ranges` are sorted and do not overlap.
    pub(crate) fn delete_ranges(&mut self, ranges: &[Range<usize>]) -> Result<(), RangeError> {
        let (Some(first), Some(last)) = (ranges.first(), ranges.last()) else {
            return Ok(());
        };
        let span = first.start..last.end;
        let mut kept = self
            .text
            .get(span.clone())
            .ok_or(RangeError {
                range: span.clone(),
            })?
            .to_owned();
        let mut slots = self.slots.clone();
        let mut ghosts = self.ghosts.clone();
        let mut marks = self.lab_marks.clone();
        for range in ranges.iter().rev() {
            let piece = range.start - span.start..range.end - span.start;
            kept.get(piece.clone()).ok_or(RangeError {
                range: range.clone(),
            })?;
            kept.replace_range(piece, "");
            slot::shift(&mut slots, &mut ghosts, range, 0, true, None);
            marks = marks
                .into_iter()
                .filter_map(|mark| mark.follow(range, 0))
                .collect();
        }
        let tree = self.tree(slots, ghosts);
        self.record(span, &kept, tree, marks)
    }

    /// The open Lab marks in document order.
    #[must_use]
    pub fn lab_marks(&self) -> &[LabMark] {
        &self.lab_marks
    }

    pub(crate) fn new_lab_mark_id(&mut self) -> LabMarkId {
        let id = self.next_lab_mark;
        self.next_lab_mark = id.next();
        id
    }

    /// Sets the Lab marks to `marks` as one undoable step. The text stays.
    pub(crate) fn commit_lab_marks(&mut self, marks: Vec<LabMark>) -> Result<(), RangeError> {
        let tree = self.tree(self.slots.clone(), self.ghosts.clone());
        self.record(0..0, "", tree, marks)
    }

    /// Replaces `range` with `inserted` and sets the Slots and Ghost ranges to `tree`,
    /// as one undoable step. The Lab marks follow the edit.
    fn commit(
        &mut self,
        range: Range<usize>,
        inserted: &str,
        tree: Tree,
    ) -> Result<(), RangeError> {
        let marks = self
            .lab_marks
            .iter()
            .filter_map(|mark| mark.clone().follow(&range, inserted.len()))
            .collect();
        self.record(range, inserted, tree, marks)
    }

    /// Replaces `range` with `inserted` and sets the Slots, Ghost ranges, and Lab marks,
    /// as one undoable step. The Trim proposal follows the edit.
    fn record(
        &mut self,
        range: Range<usize>,
        inserted: &str,
        mut tree: Tree,
        marks: Vec<LabMark>,
    ) -> Result<(), RangeError> {
        let removed = self
            .text
            .get(range.clone())
            .ok_or(RangeError {
                range: range.clone(),
            })?
            .to_owned();
        let mut text = self.text.clone();
        text.replace_range(range.clone(), inserted);
        slot::sync(&mut tree.slots, &text, 0);
        let before = Tree {
            slots: std::mem::take(&mut self.slots),
            ghosts: std::mem::take(&mut self.ghosts),
            overflow: std::mem::take(&mut self.overflow),
        };
        if text == self.text && tree == before && marks == self.lab_marks {
            self.set_tree(before);
            return Ok(());
        }
        self.text = text;
        self.set_tree(tree.clone());
        let marks_before = std::mem::replace(&mut self.lab_marks, marks);
        self.follow_trim(&range, inserted.len());
        self.undo_steps.push(Step {
            at: range.start,
            removed,
            inserted: inserted.to_owned(),
            before,
            after: tree,
            marks_before,
            marks_after: self.lab_marks.clone(),
            variant_deleted: None,
        });
        self.redo_steps.clear();
        Ok(())
    }

    /// The Overflow of the Document.
    #[must_use]
    pub fn overflow(&self) -> &Overflow {
        &self.overflow
    }

    /// Replaces the Overflow text in byte range `range` with `text`.
    ///
    /// # Errors
    /// [`RangeError`] when `range` is out of bounds, reversed, or not on char boundaries.
    pub fn edit_overflow(&mut self, range: Range<usize>, text: &str) -> Result<(), RangeError> {
        self.overflow.text().get(range.clone()).ok_or(RangeError {
            range: range.clone(),
        })?;
        let mut overflow = self.overflow.clone();
        overflow.text_mut().replace_range(range, text);
        let tree = Tree {
            slots: self.slots.clone(),
            ghosts: self.ghosts.clone(),
            overflow,
        };
        self.commit(0..0, "", tree)
    }

    /// Stash: moves the text in `range` from the Document to the end of Overflow,
    /// after a blank line, as one undoable step. The Document side is a writer delete.
    ///
    /// # Errors
    /// [`OverflowError::Range`] for an invalid range, [`OverflowError::EmptyRange`] for an empty one.
    pub fn stash(&mut self, range: Range<usize>) -> Result<(), OverflowError> {
        let stashed = self.text.get(range.clone()).ok_or(RangeError {
            range: range.clone(),
        })?;
        if stashed.is_empty() {
            return Err(OverflowError::EmptyRange);
        }
        let mut overflow = self.overflow.clone();
        overflow.append(stashed);
        let mut slots = self.slots.clone();
        let mut ghosts = self.ghosts.clone();
        slot::shift(&mut slots, &mut ghosts, &range, 0, true, None);
        let tree = Tree {
            slots,
            ghosts,
            overflow,
        };
        self.commit(range, "", tree)?;
        Ok(())
    }

    /// Uses an Overflow block: inserts the text of the block that holds Overflow
    /// offset `caret` (on its end included) at `at` in the Document, as one undoable step.
    /// The block stays in Overflow.
    ///
    /// # Errors
    /// [`OverflowError::NoBlock`] when no block holds `caret`,
    /// [`OverflowError::Range`] when `at` is out of bounds or not on a char boundary.
    pub fn use_block(&mut self, caret: usize, at: usize) -> Result<(), OverflowError> {
        let block = self
            .overflow
            .block_at(caret)
            .ok_or(OverflowError::NoBlock(caret))?
            .to_owned();
        self.insert(at, &block)?;
        Ok(())
    }

    /// How many core undo steps there are.
    pub(crate) fn undo_depth(&self) -> usize {
        self.undo_steps.len()
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
        self.set_tree(step.before.clone());
        self.lab_marks.clone_from(&step.marks_before);
        self.follow_trim(&(step.at..end), step.removed.len());
        step.inverse()
    }

    /// A Document with no edit history, as open restores it. The Active variant texts
    /// of `slots` must match their ranges of `text`.
    pub(crate) fn restored(
        text: String,
        slots: Vec<SlotNode>,
        ghosts: Vec<Range<usize>>,
        overflow: Overflow,
        lab_marks: Vec<LabMark>,
    ) -> Self {
        let next_slot = SlotId(slot::max_id(&slots)).next();
        let last_mark = lab_marks.iter().map(|mark| mark.id.0).max().unwrap_or(0);
        Self {
            text,
            slots,
            ghosts,
            overflow,
            next_slot,
            lab_marks,
            next_lab_mark: LabMarkId(last_mark).next(),
            trim: None,
            undo_steps: Vec::new(),
            redo_steps: Vec::new(),
        }
    }

    /// The top-level Slots of the tree (ADR 0006).
    pub(crate) fn slot_nodes(&self) -> &[SlotNode] {
        &self.slots
    }

    /// The Ghost ranges that are not nested in a Slot.
    pub(crate) fn top_ghosts(&self) -> &[Range<usize>] {
        &self.ghosts
    }

    /// A Tree with `slots`, `ghosts`, and the current Overflow.
    fn tree(&self, slots: Vec<SlotNode>, ghosts: Vec<Range<usize>>) -> Tree {
        Tree {
            slots,
            ghosts,
            overflow: self.overflow.clone(),
        }
    }

    fn set_tree(&mut self, tree: Tree) {
        self.slots = tree.slots;
        self.ghosts = tree.ghosts;
        self.overflow = tree.overflow;
    }
}
