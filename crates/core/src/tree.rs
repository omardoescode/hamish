//! The Tree: the Slots and Ghost ranges of one text (ADR 0006), with the operations that
//! keep them strictly nested as the text changes.

use std::ops::Range;

use crate::ghost;
use crate::slot::SlotNode;
use crate::{GhostError, Slot, SlotError, SlotId};

/// The Slots and Ghost ranges of one text: the Document, or one Variant. Ranges are relative
/// to the start of that text. Any two items are disjoint or one holds the other (ADR 0006).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Tree {
    slots: Vec<SlotNode>,
    ghosts: Vec<Range<usize>>,
}

impl Tree {
    /// The Slots of this text, not the ones nested in them, ordered by start.
    pub(crate) fn slots(&self) -> &[SlotNode] {
        &self.slots
    }

    /// The Ghost ranges of this text, not the ones nested in Slots.
    pub(crate) fn ghosts(&self) -> &[Range<usize>] {
        &self.ghosts
    }

    /// The visible Slots in text order, each parent before the Slots nested in it.
    pub(crate) fn visible_slots(&self) -> Vec<Slot<'_>> {
        let mut out = Vec::new();
        collect(&self.slots, 0, &mut out);
        out
    }

    /// The visible Ghost ranges in text order, at any depth. Nested ranges that touch join.
    pub(crate) fn ghost_ranges(&self) -> Vec<Range<usize>> {
        let mut nested = Vec::new();
        collect_ghosts(&self.slots, 0, &mut nested);
        let mut out = self.ghosts.clone();
        for range in nested {
            ghost::join(&mut out, range);
        }
        out
    }

    /// The highest Slot id in all Variants at any depth. 0 when there is none.
    pub(crate) fn max_id(&self) -> u64 {
        self.slots
            .iter()
            .map(|node| {
                let nested = node.variants.iter().map(|variant| variant.tree.max_id());
                nested.fold(node.id.0, u64::max)
            })
            .max()
            .unwrap_or(0)
    }

    /// No Slot of this text overlaps `range` (nested Slots are not checked).
    pub(crate) fn is_free(&self, range: &Range<usize>) -> bool {
        self.slots.iter().all(|kept| disjoint(&kept.range, range))
    }

    /// Adds `node` as a Slot of this text as it is. The caller checks [`Tree::is_free`].
    pub(crate) fn add(&mut self, node: SlotNode) {
        self.slots.push(node);
        self.slots.sort_by_key(|node| node.range.start);
    }

    /// Puts `node` into the deepest visible Slot that holds it.
    /// Ghost ranges inside it move into its Original; a Ghost range with the same range stays
    /// outside, so it stays Ghost whichever Variant shows.
    ///
    /// # Errors
    /// [`SlotError::PartialOverlap`] when `node` partly overlaps a Slot or a Ghost range.
    pub(crate) fn place(&mut self, mut node: SlotNode) -> Result<(), SlotError> {
        if let Some(parent) = self
            .slots
            .iter_mut()
            .find(|slot| inside(&node.range, &slot.range))
        {
            node.range = offset_back(&node.range, parent.range.start);
            return parent.active_mut()?.tree.place(node);
        }
        let overlaps = self
            .slots
            .iter()
            .map(|slot| &slot.range)
            .chain(&self.ghosts);
        if overlaps
            .into_iter()
            .any(|range| overlaps_partly(range, &node.range))
        {
            return Err(SlotError::PartialOverlap);
        }
        let (mut inner, outer): (Vec<SlotNode>, Vec<SlotNode>) = std::mem::take(&mut self.slots)
            .into_iter()
            .partition(|slot| inside(&slot.range, &node.range));
        for child in &mut inner {
            child.range = offset_back(&child.range, node.range.start);
        }
        let (inner_ghosts, outer_ghosts): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ghosts)
            .into_iter()
            .partition(|ghost| *ghost != node.range && inside(ghost, &node.range));
        let start = node.range.start;
        node.active_mut()?.tree = Self {
            slots: inner,
            ghosts: inner_ghosts
                .iter()
                .map(|ghost| offset_back(ghost, start))
                .collect(),
        };
        self.slots = outer;
        self.ghosts = outer_ghosts;
        self.add(node);
        Ok(())
    }

    /// Ghosts `range` in the Active variant of the deepest Slot that holds it. A Ghost range
    /// with the same range as a Slot stays outside it. Ghost ranges that touch it join it.
    ///
    /// # Errors
    /// [`GhostError::PartialOverlap`] when `range` partly overlaps a Slot.
    pub(crate) fn place_ghost(&mut self, range: Range<usize>) -> Result<(), GhostError> {
        if let Some(parent) = self
            .slots
            .iter_mut()
            .find(|slot| slot.range != range && inside(&range, &slot.range))
        {
            let local = offset_back(&range, parent.range.start);
            let Ok(active) = parent.active_mut() else {
                return Err(GhostError::PartialOverlap);
            };
            return active.tree.place_ghost(local);
        }
        if self
            .slots
            .iter()
            .any(|slot| overlaps_partly(&slot.range, &range))
        {
            return Err(GhostError::PartialOverlap);
        }
        ghost::join(&mut self.ghosts, range);
        Ok(())
    }

    /// Revives the visible Ghost ranges that `range` touches, at any depth. Returns how many.
    pub(crate) fn revive(&mut self, range: &Range<usize>) -> usize {
        let mut count = ghost::revive(&mut self.ghosts, range);
        for node in &mut self.slots {
            let start = node.range.start;
            let local = range.start.saturating_sub(start)..range.end.saturating_sub(start);
            let touches = range.start <= node.range.end && node.range.start <= range.end;
            if let (true, Ok(active)) = (touches, node.active_mut()) {
                count += active.tree.revive(&local);
            }
        }
        count
    }

    /// Finds visible Slot `id`. Returns it and the start of the text that holds it.
    pub(crate) fn find_mut(&mut self, id: SlotId) -> Option<(&mut SlotNode, usize)> {
        find_mut(&mut self.slots, id, 0)
    }

    /// Ends visible Slot `id`: the Slots and Ghost ranges nested in its Active variant
    /// take its place.
    pub(crate) fn dissolve(&mut self, id: SlotId) {
        let Some(position) = self.slots.iter().position(|node| node.id == id) else {
            for node in &mut self.slots {
                if let Ok(active) = node.active_mut() {
                    active.tree.dissolve(id);
                }
            }
            return;
        };
        let mut node = self.slots.remove(position);
        let start = node.range.start;
        let nested = node
            .active_mut()
            .map_or_default(|active| std::mem::take(&mut active.tree));
        self.slots.extend(nested.slots.into_iter().map(|mut child| {
            child.range = child.range.start + start..child.range.end + start;
            child
        }));
        self.slots.sort_by_key(|node| node.range.start);
        for inner in nested.ghosts {
            ghost::join(&mut self.ghosts, inner.start + start..inner.end + start);
        }
    }

    /// Moves, trims, or removes Slots and Ghost ranges for a text edit: `edit` was replaced by
    /// `inserted` bytes. `writer_edit`: the writer changed the text, so an AI Variant that
    /// holds the edit becomes Mixed. Slot `except` is left as it is. A Slot inside the removed
    /// text ends. A Ghost range left empty ends.
    pub(crate) fn shift(
        &mut self,
        edit: &Range<usize>,
        inserted: usize,
        writer_edit: bool,
        except: Option<SlotId>,
    ) {
        let removed = edit.len();
        for ghost in &mut self.ghosts {
            *ghost = match relation(ghost, edit) {
                Relation::Before => {
                    ghost.start + inserted - removed..ghost.end + inserted - removed
                }
                Relation::After => ghost.clone(),
                Relation::Within => ghost.start..ghost.end + inserted - removed,
                Relation::PartialLeft => edit.start + inserted..ghost.end + inserted - removed,
                Relation::PartialRight => ghost.start..edit.start,
                Relation::Covered => 0..0,
            };
        }
        self.ghosts.retain(|ghost| !ghost.is_empty());
        self.slots.retain(|node| {
            Some(node.id) == except || relation(&node.range, edit) != Relation::Covered
        });
        for node in self.slots.iter_mut().filter(|node| Some(node.id) != except) {
            node.follow(edit, inserted, writer_edit, except);
        }
    }

    /// Sets each visible Active variant text to its range of `text`.
    pub(crate) fn sync(&mut self, text: &str) {
        sync(&mut self.slots, text, 0);
    }
}

impl SlotNode {
    /// Moves or trims this Slot for a text edit at `edit` (relative to the text that holds it).
    fn follow(
        &mut self,
        edit: &Range<usize>,
        inserted: usize,
        writer_edit: bool,
        except: Option<SlotId>,
    ) {
        let removed = edit.len();
        let relation = if except.is_some_and(|id| self.holds(id)) {
            Relation::Within
        } else {
            relation(&self.range, edit)
        };
        match relation {
            Relation::Before => {
                self.range =
                    self.range.start + inserted - removed..self.range.end + inserted - removed;
            }
            Relation::After | Relation::Covered => {}
            Relation::Within => self.take_within(edit, inserted, writer_edit, except),
            Relation::PartialLeft => self.lose_left(edit, inserted),
            Relation::PartialRight => self.lose_right(edit),
        }
    }

    fn take_within(
        &mut self,
        edit: &Range<usize>,
        inserted: usize,
        writer_edit: bool,
        except: Option<SlotId>,
    ) {
        let local = offset_back(edit, self.range.start);
        if let Ok(active) = self.active_mut() {
            active.tree.shift(&local, inserted, writer_edit, except);
        }
        self.range.end = self.range.end + inserted - edit.len();
        self.mark_edited(writer_edit);
    }

    fn lose_left(&mut self, edit: &Range<usize>, inserted: usize) {
        let lost = 0..edit.end - self.range.start;
        if let Ok(active) = self.active_mut() {
            active.tree.shift(&lost, 0, true, None);
        }
        self.range = edit.start + inserted..self.range.end + inserted - edit.len();
        self.mark_edited(true);
    }

    fn lose_right(&mut self, edit: &Range<usize>) {
        let lost = edit.start - self.range.start..self.range.end - self.range.start;
        if let Ok(active) = self.active_mut() {
            active.tree.shift(&lost, 0, true, None);
        }
        self.range.end = edit.start;
        self.mark_edited(true);
    }

    fn mark_edited(&mut self, writer_edit: bool) {
        let active = self.active;
        let Some(variant) = self.variants.get_mut(active) else {
            return;
        };
        if writer_edit {
            variant.mark_edited();
        }
    }

    /// Slot `id` is nested in this Slot's active text, at any depth.
    fn holds(&self, id: SlotId) -> bool {
        self.nested()
            .slots
            .iter()
            .any(|child| child.id == id || child.holds(id))
    }

    /// The Tree of the Active variant; an empty Tree when there is no such Variant.
    fn nested(&self) -> &Tree {
        static EMPTY: Tree = Tree {
            slots: Vec::new(),
            ghosts: Vec::new(),
        };
        self.variants
            .get(self.active)
            .map_or(&EMPTY, |variant| &variant.tree)
    }
}

fn collect<'a>(slots: &'a [SlotNode], base: usize, out: &mut Vec<Slot<'a>>) {
    for node in slots {
        let start = base + node.range.start;
        out.push(Slot {
            id: node.id,
            level: node.level,
            range: start..base + node.range.end,
            variants: &node.variants,
            active: node.active,
        });
        collect(&node.nested().slots, start, out);
    }
}

fn collect_ghosts(slots: &[SlotNode], base: usize, out: &mut Vec<Range<usize>>) {
    for node in slots {
        let start = base + node.range.start;
        let nested = node.nested();
        out.extend(
            nested
                .ghosts
                .iter()
                .map(|ghost| start + ghost.start..start + ghost.end),
        );
        collect_ghosts(&nested.slots, start, out);
    }
}

fn find_mut(slots: &mut [SlotNode], id: SlotId, base: usize) -> Option<(&mut SlotNode, usize)> {
    for node in slots {
        if node.id == id {
            return Some((node, base));
        }
        let start = base + node.range.start;
        let found = node
            .active_mut()
            .ok()
            .and_then(|active| find_mut(&mut active.tree.slots, id, start));
        if found.is_some() {
            return found;
        }
    }
    None
}

fn sync(slots: &mut [SlotNode], text: &str, base: usize) {
    for node in slots {
        let start = base + node.range.start;
        let active_text = text.get(start..base + node.range.end);
        let active = node.active;
        if let (Some(active_text), Some(variant)) = (active_text, node.variants.get_mut(active))
            && variant.text != active_text
        {
            active_text.clone_into(&mut variant.text);
        }
        if let Ok(active) = node.active_mut() {
            sync(&mut active.tree.slots, text, start);
        }
    }
}

/// Where a text edit lies, seen from a Slot or Ghost range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Relation {
    /// Ends at or before the range start (an insert at the start goes before the range).
    Before,
    /// Starts at or after the range end (an insert at the end goes after the range).
    After,
    Within,
    /// Removes the start of the range text.
    PartialLeft,
    /// Removes the end of the range text.
    PartialRight,
    /// Removes all the range text and more, or an empty range inside it.
    Covered,
}

fn relation(slot: &Range<usize>, edit: &Range<usize>) -> Relation {
    if edit.end <= slot.start {
        return Relation::Before;
    }
    if edit.start >= slot.end {
        return Relation::After;
    }
    if slot.start <= edit.start && edit.end <= slot.end {
        return Relation::Within;
    }
    if edit.start < slot.start && edit.end < slot.end {
        return Relation::PartialLeft;
    }
    if slot.start < edit.start && slot.end < edit.end {
        return Relation::PartialRight;
    }
    Relation::Covered
}

fn offset_back(range: &Range<usize>, by: usize) -> Range<usize> {
    range.start - by..range.end - by
}

/// `inner` lies in `outer`. An empty `inner` on an edge of `outer` lies outside it.
fn inside(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    let on_edge = inner.is_empty() && (inner.start == outer.start || inner.start == outer.end);
    outer.start <= inner.start && inner.end <= outer.end && !on_edge
}

fn disjoint(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.end <= b.start || b.end <= a.start
}

fn overlaps_partly(a: &Range<usize>, b: &Range<usize>) -> bool {
    !disjoint(a, b) && !inside(a, b) && !inside(b, a)
}

#[cfg(test)]
mod tests;
