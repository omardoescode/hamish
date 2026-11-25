//! The Document: its text, Slots, Ghost ranges, Overflow, Lab marks, and Trim proposal,
//! with core undo and redo.

use std::fmt;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use crate::article;
use crate::markdown;
use crate::slot::SlotNode;
use crate::tree::Tree;
use crate::undo::{Snapshot, Step, UndoSteps};
use crate::{
    Counter, Cycle, GhostError, LabMark, LabMarkId, Level, MarkdownSpan, Overflow, OverflowError,
    Provenance, Slot, SlotError, SlotId, Switch, TrimProposal, Variant, paragraph_around,
};

/// One piece of writing: its text, Slots, Ghost ranges, Overflow, Lab marks, Trim proposal,
/// and undo steps.
#[derive(Debug, Clone, Default)]
pub struct Document {
    text: String,
    /// The Tree, Overflow, and Lab marks; undo steps share it.
    state: Arc<Snapshot>,
    next_slot: SlotId,
    next_lab_mark: LabMarkId,
    pub(crate) trim: Option<TrimProposal>,
    steps: UndoSteps,
    derived: Derived,
}

/// Values that follow from the text and the Tree, made at the first ask after a change.
#[derive(Debug, Clone, Default)]
struct Derived {
    counter: OnceLock<Counter>,
    markdown: OnceLock<Vec<MarkdownSpan>>,
}

/// How a Slot change reaches the text. Only a writer text edit makes an
/// outer AI Variant Mixed; a Variant switch does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotChange {
    Switch,
    WriterText,
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
    /// A Document with `text`, and no Slots, Ghost ranges, Overflow, or undo steps.
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
        *self
            .derived
            .counter
            .get_or_init(|| self.count_outside(0..self.text.len(), &[]))
    }

    /// The visible Ghost ranges in document order, as byte ranges of the active text.
    #[must_use]
    pub fn ghost_ranges(&self) -> Vec<Range<usize>> {
        self.state.tree.ghost_ranges()
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
        let mut tree = self.state.tree.clone();
        tree.place_ghost(range.clone())?;
        self.commit(range.start..range.start, "", tree)?;
        Ok(())
    }

    /// Revives each visible Ghost range that `range` touches. An empty `range`
    /// (a caret) touches a Ghost range it lies in or on an edge of.
    ///
    /// # Errors
    /// [`GhostError::NoGhost`] when `range` touches no Ghost range.
    pub fn revive(&mut self, range: Range<usize>) -> Result<(), GhostError> {
        let mut tree = self.state.tree.clone();
        if tree.revive(&range) == 0 {
            return Err(GhostError::NoGhost(range));
        }
        self.commit(range.start..range.start, "", tree)?;
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
        self.derived
            .markdown
            .get_or_init(|| markdown::spans(&self.text))
            .clone()
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
        self.state.tree.visible_slots()
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
        self.slot_over(&(caret..caret))
    }

    /// The innermost visible Slot whose span holds all of `range`, edges included.
    #[must_use]
    pub fn slot_over(&self, range: &Range<usize>) -> Option<SlotId> {
        self.slots()
            .iter()
            .rev()
            .find(|slot| slot.range.start <= range.start && range.end <= slot.range.end)
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
        let (id, tree) = self.placed_slot(range.clone(), level, texts, provenance)?;
        self.commit(range.start..range.start, "", tree)?;
        self.next_slot = id.next();
        Ok(id)
    }

    /// Makes a Slot over `range` with `text` as the Active variant after the Original, as
    /// one undoable step, with Article agreement as in [`Document::set_active`].
    ///
    /// # Errors
    /// As [`Document::create_slot`].
    pub fn create_active_slot(
        &mut self,
        range: Range<usize>,
        level: Level,
        text: &str,
        provenance: Provenance,
    ) -> Result<SlotId, SlotError> {
        let (id, tree) = self.placed_slot(range, level, &[text], provenance)?;
        self.change_slot_in(tree, id, SlotChange::Switch, |node| node.set_active(1))?;
        self.next_slot = id.next();
        Ok(id)
    }

    /// The Tree with a new Slot over `range` placed, and its id.
    fn placed_slot(
        &self,
        range: Range<usize>,
        level: Level,
        texts: &[&str],
        provenance: Provenance,
    ) -> Result<(SlotId, Tree), SlotError> {
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
        let mut tree = self.state.tree.clone();
        tree.place(node)?;
        Ok((id, tree))
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

    /// Adds a Variant at the end of Slot `id` and makes it the Active variant, as one
    /// undoable step, with Article agreement as in [`Document::set_active`].
    ///
    /// # Errors
    /// [`SlotError::NoSlot`] when `id` is not a visible Slot.
    pub fn add_active_variant(
        &mut self,
        id: SlotId,
        text: &str,
        provenance: Provenance,
    ) -> Result<Switch, SlotError> {
        self.change_slot(id, SlotChange::Switch, |node| {
            node.push_variant(text, provenance);
            node.set_active(node.variants.len().saturating_sub(1))
        })?;
        Ok(Switch::ToVariant)
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
        let depth = self.steps.depth();
        let kept = self.change_slot(id, SlotChange::Switch, |node| node.remove_variant(index))?;
        self.steps.mark_variant_deleted(depth, id);
        Ok(kept)
    }

    /// The Slot whose Variant the next [`Document::undo`] puts back: `Some` when the last
    /// step is a Variant delete.
    #[must_use]
    pub fn undo_restores_variant(&self) -> Option<SlotId> {
        self.steps.undo_restores_variant()
    }

    /// The Slot whose Variant the next [`Document::redo`] deletes again: `Some` when the
    /// last undone step is a Variant delete.
    #[must_use]
    pub fn redo_deletes_variant(&self) -> Option<SlotId> {
        self.steps.redo_deletes_variant()
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
        self.change_slot_in(self.state.tree.clone(), id, kind, change)
    }

    /// Like [`Self::change_slot`], on `tree` in place of the current Tree.
    fn change_slot_in(
        &mut self,
        mut tree: Tree,
        id: SlotId,
        kind: SlotChange,
        change: impl FnOnce(&mut SlotNode) -> Result<(), SlotError>,
    ) -> Result<Option<Range<usize>>, SlotError> {
        let (node, base) = tree.find_mut(id).ok_or(SlotError::NoSlot(id))?;
        let old = base + node.range.start..base + node.range.end;
        change(node)?;
        let shown = node.active_text()?.to_owned();
        node.range.end = node.range.start + shown.len();
        let shown_changed = self.text.get(old.clone()) != Some(shown.as_str());
        let writer_edit = kind == SlotChange::WriterText && shown_changed;
        let agrees = node.level == Level::Word && shown_changed;
        let ends = node.variants.len() < 2;
        tree.shift(&old, shown.len(), writer_edit, Some(id));
        if ends {
            tree.dissolve(id);
        }
        let Some((article_range, article)) = agrees
            .then(|| self.article_agreement(id, &old, &shown))
            .flatten()
        else {
            let kept = ends.then(|| old.start..old.start + shown.len());
            self.commit(old, &shown, tree)?;
            return Ok(kept);
        };
        tree.shift(&article_range, article.len(), false, None);
        let gap = self
            .text
            .get(article_range.end..old.start)
            .unwrap_or_default();
        let inserted = format!("{article}{gap}{shown}");
        let start = old.start - article_range.len() + article.len();
        let kept = ends.then(|| start..start + shown.len());
        self.commit(article_range.start..old.end, &inserted, tree)?;
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
        let mut tree = self.state.tree.clone();
        tree.shift(&range, text.len(), writer_edit, None);
        self.commit(range, text, tree)
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
        let mut tree = self.state.tree.clone();
        let mut marks = self.state.lab_marks.clone();
        for range in ranges.iter().rev() {
            let piece = range.start - span.start..range.end - span.start;
            kept.get(piece.clone()).ok_or(RangeError {
                range: range.clone(),
            })?;
            kept.replace_range(piece, "");
            tree.shift(range, 0, true, None);
            marks = marks
                .into_iter()
                .filter_map(|mark| mark.follow(range, 0))
                .collect();
        }
        let after = Snapshot {
            tree,
            overflow: self.state.overflow.clone(),
            lab_marks: marks,
        };
        self.record(span, &kept, after)
    }

    /// The open Lab marks in document order.
    #[must_use]
    pub fn lab_marks(&self) -> &[LabMark] {
        &self.state.lab_marks
    }

    pub(crate) fn new_lab_mark_id(&mut self) -> LabMarkId {
        let id = self.next_lab_mark;
        self.next_lab_mark = id.next();
        id
    }

    /// Sets the Lab marks to `marks` as one undoable step. The text stays.
    pub(crate) fn commit_lab_marks(&mut self, marks: Vec<LabMark>) -> Result<(), RangeError> {
        let after = Snapshot {
            tree: self.state.tree.clone(),
            overflow: self.state.overflow.clone(),
            lab_marks: marks,
        };
        self.record(0..0, "", after)
    }

    /// Replaces `range` with `inserted` and sets the Tree to `tree`, as one undoable step.
    /// The Lab marks follow the edit.
    fn commit(
        &mut self,
        range: Range<usize>,
        inserted: &str,
        tree: Tree,
    ) -> Result<(), RangeError> {
        self.commit_with(range, inserted, tree, self.state.overflow.clone())
    }

    /// Like [`Self::commit`], and sets the Overflow to `overflow`.
    fn commit_with(
        &mut self,
        range: Range<usize>,
        inserted: &str,
        tree: Tree,
        overflow: Overflow,
    ) -> Result<(), RangeError> {
        let lab_marks = self
            .state
            .lab_marks
            .iter()
            .filter_map(|mark| mark.clone().follow(&range, inserted.len()))
            .collect();
        let after = Snapshot {
            tree,
            overflow,
            lab_marks,
        };
        self.record(range, inserted, after)
    }

    /// Replaces `range` with `inserted` and sets the Tree, Overflow, and Lab marks to `after`,
    /// as one undoable step. The Trim proposal follows the edit.
    fn record(
        &mut self,
        range: Range<usize>,
        inserted: &str,
        mut after: Snapshot,
    ) -> Result<(), RangeError> {
        let removed = self
            .text
            .get(range.clone())
            .ok_or(RangeError {
                range: range.clone(),
            })?
            .to_owned();
        self.text.replace_range(range.clone(), inserted);
        after.tree.sync(&self.text);
        if removed == inserted && after == *self.state {
            return Ok(());
        }
        let after = Arc::new(after);
        let before = std::mem::replace(&mut self.state, Arc::clone(&after));
        self.derived = Derived::default();
        self.follow_trim(&range, inserted.len());
        self.steps.push(Step {
            at: range.start,
            removed,
            inserted: inserted.to_owned(),
            before,
            after,
            variant_deleted: None,
        });
        Ok(())
    }

    /// The Overflow of the Document.
    #[must_use]
    pub fn overflow(&self) -> &Overflow {
        &self.state.overflow
    }

    /// Replaces the Overflow text in byte range `range` with `text`.
    ///
    /// # Errors
    /// [`RangeError`] when `range` is out of bounds, reversed, or not on char boundaries.
    pub fn edit_overflow(&mut self, range: Range<usize>, text: &str) -> Result<(), RangeError> {
        self.state
            .overflow
            .text()
            .get(range.clone())
            .ok_or(RangeError {
                range: range.clone(),
            })?;
        let mut overflow = self.state.overflow.clone();
        overflow.text_mut().replace_range(range, text);
        self.commit_with(0..0, "", self.state.tree.clone(), overflow)
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
        let mut overflow = self.state.overflow.clone();
        overflow.append(stashed);
        let mut tree = self.state.tree.clone();
        tree.shift(&range, 0, true, None);
        self.commit_with(range, "", tree, overflow)?;
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
            .state
            .overflow
            .block_at(caret)
            .ok_or(OverflowError::NoBlock(caret))?
            .to_owned();
        self.insert(at, &block)?;
        Ok(())
    }

    /// [`Document::use_block`] for a block dropped at `at`: a space goes between the block
    /// and a word char on each side. No space goes before punctuation or next to whitespace.
    ///
    /// # Errors
    /// As [`Document::use_block`].
    pub fn drop_block(&mut self, caret: usize, at: usize) -> Result<(), OverflowError> {
        let block = self
            .state
            .overflow
            .block_at(caret)
            .ok_or(OverflowError::NoBlock(caret))?;
        let before = self
            .text
            .get(..at)
            .and_then(|text| text.chars().next_back());
        let after = self.text.get(at..).and_then(|text| text.chars().next());
        let lead =
            before.is_some_and(|c| !c.is_whitespace()) && !block.starts_with(char::is_whitespace);
        let trail =
            after.is_some_and(char::is_alphanumeric) && !block.ends_with(char::is_whitespace);
        let spaced = format!(
            "{}{block}{}",
            if lead { " " } else { "" },
            if trail { " " } else { "" }
        );
        self.insert(at, &spaced)?;
        Ok(())
    }

    /// How many core undo steps there are.
    pub(crate) fn undo_depth(&self) -> usize {
        self.steps.depth()
    }

    /// Reverts the last step. `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(step) = self.steps.take_undo() else {
            return false;
        };
        let step = self.apply(step);
        self.steps.undone(step);
        true
    }

    /// Applies the last undone step again. `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(step) = self.steps.take_redo() else {
            return false;
        };
        let step = self.apply(step);
        self.steps.redone(step);
        true
    }

    /// Reverts `step` and returns the step that reverts it back.
    fn apply(&mut self, step: Step) -> Step {
        let inserted = step.inserted_range();
        self.text.replace_range(inserted.clone(), &step.removed);
        self.state = Arc::clone(&step.before);
        self.derived = Derived::default();
        self.follow_trim(&inserted, step.removed.len());
        step.inverse()
    }

    /// A Document with no undo steps, as open restores it. The Active variant texts
    /// of `tree` must match their ranges of `text`.
    pub(crate) fn restored(
        text: String,
        tree: Tree,
        overflow: Overflow,
        lab_marks: Vec<LabMark>,
    ) -> Self {
        let next_slot = SlotId(tree.max_id()).next();
        let last_mark = lab_marks.iter().map(|mark| mark.id.0).max().unwrap_or(0);
        Self {
            text,
            state: Arc::new(Snapshot {
                tree,
                overflow,
                lab_marks,
            }),
            next_slot,
            next_lab_mark: LabMarkId(last_mark).next(),
            trim: None,
            steps: UndoSteps::default(),
            derived: Derived::default(),
        }
    }

    /// The Slots and Ghost ranges of the Document text (ADR 0006).
    pub(crate) fn tree(&self) -> &Tree {
        &self.state.tree
    }

    /// A copy of this Document with no undo steps, for a save or a preview.
    #[must_use]
    pub fn without_history(&self) -> Self {
        Self {
            text: self.text.clone(),
            state: Arc::clone(&self.state),
            next_slot: self.next_slot,
            next_lab_mark: self.next_lab_mark,
            trim: self.trim.clone(),
            steps: UndoSteps::default(),
            derived: self.derived.clone(),
        }
    }
}
