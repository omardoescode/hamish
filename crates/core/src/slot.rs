use std::cmp::Ordering;
use std::fmt;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::ghost;
use crate::{GhostError, RangeError};

/// The granularity of a Slot. Word also covers short phrases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Word,
    Sentence,
    Paragraph,
}

/// Who wrote a Variant. Mixed: an AI Variant the writer edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Writer,
    Ai,
    Mixed,
}

/// Direction to cycle the Active variant. Both wrap around.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cycle {
    Next,
    Previous,
}

/// Stable name of a Slot for the life of its Document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotId(pub(crate) u64);

impl SlotId {
    pub(crate) fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for SlotId {
    fn default() -> Self {
        Self(1)
    }
}

/// One candidate text for a Slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub(crate) text: String,
    pub(crate) provenance: Provenance,
    pub(crate) slots: Vec<SlotNode>,
    pub(crate) ghosts: Vec<Range<usize>>,
}

impl Variant {
    pub(crate) fn new(text: &str, provenance: Provenance) -> Self {
        Self {
            text: text.to_owned(),
            provenance,
            slots: Vec::new(),
            ghosts: Vec::new(),
        }
    }

    /// A writer edit to the text of an AI Variant makes it Mixed.
    fn mark_edited(&mut self) {
        if self.provenance == Provenance::Ai {
            self.provenance = Provenance::Mixed;
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn provenance(&self) -> Provenance {
        self.provenance
    }
}

/// A Slot as the Document shows it now. `range` is a byte range of the active text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot<'a> {
    pub id: SlotId,
    pub level: Level,
    pub range: Range<usize>,
    /// The Original first, then the other Variants in the order they were added.
    pub variants: &'a [Variant],
    /// Index of the Active variant in `variants`.
    pub active: usize,
}

/// Why a Slot operation was refused. A refused operation changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotError {
    Range(RangeError),
    EmptyRange,
    /// The range partly overlaps another Slot or a Ghost range (strict nesting, ADR 0006).
    PartialOverlap,
    NoSlot(SlotId),
    NoVariant {
        slot: SlotId,
        index: usize,
    },
}

impl fmt::Display for SlotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range(error) => error.fmt(f),
            Self::EmptyRange => f.write_str("a Slot needs a non-empty range"),
            Self::PartialOverlap => {
                f.write_str("the range partly overlaps another Slot or a Ghost range")
            }
            Self::NoSlot(id) => write!(f, "no Slot {}", id.0),
            Self::NoVariant { slot, index } => write!(f, "Slot {} has no Variant {index}", slot.0),
        }
    }
}

impl std::error::Error for SlotError {}

impl From<RangeError> for SlotError {
    fn from(error: RangeError) -> Self {
        Self::Range(error)
    }
}

/// A Slot in the tree. `range` is relative to the start of the text that holds it:
/// the Document for a top-level Slot, else the Active variant of the parent Slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotNode {
    pub(crate) id: SlotId,
    pub(crate) level: Level,
    pub(crate) range: Range<usize>,
    pub(crate) variants: Vec<Variant>,
    pub(crate) active: usize,
}

impl SlotNode {
    pub(crate) fn active_text(&self) -> Result<&str, SlotError> {
        self.variants
            .get(self.active)
            .map(|variant| variant.text.as_str())
            .ok_or(self.no_variant(self.active))
    }

    pub(crate) fn no_variant(&self, index: usize) -> SlotError {
        SlotError::NoVariant {
            slot: self.id,
            index,
        }
    }

    pub(crate) fn set_active(&mut self, index: usize) -> Result<(), SlotError> {
        if index >= self.variants.len() {
            return Err(self.no_variant(index));
        }
        self.active = index;
        Ok(())
    }

    pub(crate) fn push_variant(&mut self, text: &str, provenance: Provenance) {
        self.variants.push(Variant::new(text, provenance));
    }

    /// Removes Variant `index`. When it was active, the next Variant becomes active;
    /// after the last one, the first (FR-77).
    pub(crate) fn remove_variant(&mut self, index: usize) -> Result<(), SlotError> {
        if index >= self.variants.len() {
            return Err(self.no_variant(index));
        }
        self.variants.remove(index);
        self.active = match index.cmp(&self.active) {
            Ordering::Less => self.active - 1,
            Ordering::Equal if index < self.variants.len() => index,
            Ordering::Equal => 0,
            Ordering::Greater => self.active,
        };
        Ok(())
    }

    /// A writer edit of Variant `index`. The Slots and Ghost ranges nested in it end.
    pub(crate) fn edit_variant(&mut self, index: usize, text: &str) -> Result<(), SlotError> {
        let error = self.no_variant(index);
        let variant = self.variants.get_mut(index).ok_or(error)?;
        if variant.text == text {
            return Ok(());
        }
        text.clone_into(&mut variant.text);
        variant.slots.clear();
        variant.ghosts.clear();
        variant.mark_edited();
        Ok(())
    }

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
            shift(
                &mut active.slots,
                &mut active.ghosts,
                &local,
                inserted,
                writer_edit,
                except,
            );
        }
        self.range.end = self.range.end + inserted - edit.len();
        self.mark_edited(writer_edit);
    }

    fn lose_left(&mut self, edit: &Range<usize>, inserted: usize) {
        let lost = 0..edit.end - self.range.start;
        if let Ok(active) = self.active_mut() {
            shift(&mut active.slots, &mut active.ghosts, &lost, 0, true, None);
        }
        self.range = edit.start + inserted..self.range.end + inserted - edit.len();
        self.mark_edited(true);
    }

    fn lose_right(&mut self, edit: &Range<usize>) {
        let lost = edit.start - self.range.start..self.range.end - self.range.start;
        if let Ok(active) = self.active_mut() {
            shift(&mut active.slots, &mut active.ghosts, &lost, 0, true, None);
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
        self.children()
            .iter()
            .any(|child| child.id == id || child.holds(id))
    }

    fn children(&self) -> &[SlotNode] {
        self.variants
            .get(self.active)
            .map_or(&[], |variant| variant.slots.as_slice())
    }

    fn ghosts(&self) -> &[Range<usize>] {
        self.variants
            .get(self.active)
            .map_or(&[], |variant| variant.ghosts.as_slice())
    }

    fn active_mut(&mut self) -> Result<&mut Variant, SlotError> {
        let error = self.no_variant(self.active);
        self.variants.get_mut(self.active).ok_or(error)
    }
}

/// The highest Slot id in `slots`, in all their Variants at any depth. 0 when there is none.
pub(crate) fn max_id(slots: &[SlotNode]) -> u64 {
    slots
        .iter()
        .map(|node| {
            let nested = node.variants.iter().map(|variant| max_id(&variant.slots));
            nested.fold(node.id.0, u64::max)
        })
        .max()
        .unwrap_or(0)
}

/// Appends the Slots in `slots` and their visible descendants, parents first.
pub(crate) fn collect<'a>(slots: &'a [SlotNode], base: usize, out: &mut Vec<Slot<'a>>) {
    for node in slots {
        let start = base + node.range.start;
        out.push(Slot {
            id: node.id,
            level: node.level,
            range: start..base + node.range.end,
            variants: &node.variants,
            active: node.active,
        });
        collect(node.children(), start, out);
    }
}

/// Puts `node` (range relative to the text that holds `slots` and `ghosts`) into the tree.
/// Ghost ranges inside it move into its Original; a Ghost range with the same range stays
/// outside, so it stays Ghost whichever Variant shows.
pub(crate) fn place(
    slots: &mut Vec<SlotNode>,
    ghosts: &mut Vec<Range<usize>>,
    mut node: SlotNode,
) -> Result<(), SlotError> {
    if let Some(parent) = slots
        .iter_mut()
        .find(|slot| inside(&node.range, &slot.range))
    {
        node.range = offset_back(&node.range, parent.range.start);
        let active = parent.active_mut()?;
        return place(&mut active.slots, &mut active.ghosts, node);
    }
    let overlaps = slots.iter().map(|slot| &slot.range).chain(ghosts.iter());
    if overlaps
        .into_iter()
        .any(|range| overlaps_partly(range, &node.range))
    {
        return Err(SlotError::PartialOverlap);
    }
    let (mut inner, outer): (Vec<SlotNode>, Vec<SlotNode>) = std::mem::take(slots)
        .into_iter()
        .partition(|slot| inside(&slot.range, &node.range));
    for child in &mut inner {
        child.range = offset_back(&child.range, node.range.start);
    }
    let (inner_ghosts, outer_ghosts): (Vec<_>, Vec<_>) = std::mem::take(ghosts)
        .into_iter()
        .partition(|ghost| *ghost != node.range && inside(ghost, &node.range));
    let start = node.range.start;
    let active = node.active_mut()?;
    active.slots = inner;
    active.ghosts = inner_ghosts
        .iter()
        .map(|ghost| offset_back(ghost, start))
        .collect();
    *slots = outer;
    *ghosts = outer_ghosts;
    slots.push(node);
    slots.sort_by_key(|slot| slot.range.start);
    Ok(())
}

/// Ghosts `range` (relative to the text that holds `slots` and `ghosts`) in the
/// Active variant of the deepest Slot that holds it. A Ghost range with the same range
/// as a Slot stays outside it.
pub(crate) fn place_ghost(
    slots: &mut [SlotNode],
    ghosts: &mut Vec<Range<usize>>,
    range: Range<usize>,
) -> Result<(), GhostError> {
    if let Some(parent) = slots
        .iter_mut()
        .find(|slot| slot.range != range && inside(&range, &slot.range))
    {
        let local = offset_back(&range, parent.range.start);
        let Ok(active) = parent.active_mut() else {
            return Err(GhostError::PartialOverlap);
        };
        return place_ghost(&mut active.slots, &mut active.ghosts, local);
    }
    if slots
        .iter()
        .any(|slot| overlaps_partly(&slot.range, &range))
    {
        return Err(GhostError::PartialOverlap);
    }
    ghost::join(ghosts, range);
    Ok(())
}

/// Revives the visible Ghost ranges that `range` touches, at any depth. Returns how many.
pub(crate) fn revive(
    slots: &mut [SlotNode],
    ghosts: &mut Vec<Range<usize>>,
    range: &Range<usize>,
) -> usize {
    let mut count = ghost::revive(ghosts, range);
    for node in slots {
        let start = node.range.start;
        let local = range.start.saturating_sub(start)..range.end.saturating_sub(start);
        let touches = range.start <= node.range.end && node.range.start <= range.end;
        if let (true, Ok(active)) = (touches, node.active_mut()) {
            count += revive(&mut active.slots, &mut active.ghosts, &local);
        }
    }
    count
}

/// Appends the visible Ghost ranges nested in `slots`, at any depth, as ranges of the text
/// that holds `slots`, plus `base`.
pub(crate) fn collect_ghosts(slots: &[SlotNode], base: usize, out: &mut Vec<Range<usize>>) {
    for node in slots {
        let start = base + node.range.start;
        out.extend(
            node.ghosts()
                .iter()
                .map(|ghost| start + ghost.start..start + ghost.end),
        );
        collect_ghosts(node.children(), start, out);
    }
}

/// Finds visible Slot `id`. Returns it and the start of the text that holds it.
pub(crate) fn find_mut(
    slots: &mut [SlotNode],
    id: SlotId,
    base: usize,
) -> Option<(&mut SlotNode, usize)> {
    for node in slots {
        if node.id == id {
            return Some((node, base));
        }
        let start = base + node.range.start;
        let found = node
            .active_mut()
            .ok()
            .and_then(|active| find_mut(&mut active.slots, id, start));
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Ends visible Slot `id`: the Slots and Ghost ranges nested in its Active variant
/// take its place.
pub(crate) fn dissolve(slots: &mut Vec<SlotNode>, ghosts: &mut Vec<Range<usize>>, id: SlotId) {
    let Some(position) = slots.iter().position(|node| node.id == id) else {
        for node in slots.iter_mut() {
            if let Ok(active) = node.active_mut() {
                dissolve(&mut active.slots, &mut active.ghosts, id);
            }
        }
        return;
    };
    let mut node = slots.remove(position);
    let start = node.range.start;
    let (children, inner_ghosts) = node.active_mut().map_or_default(|active| {
        (
            std::mem::take(&mut active.slots),
            std::mem::take(&mut active.ghosts),
        )
    });
    slots.extend(children.into_iter().map(|mut child| {
        child.range = child.range.start + start..child.range.end + start;
        child
    }));
    slots.sort_by_key(|node| node.range.start);
    for inner in inner_ghosts {
        ghost::join(ghosts, inner.start + start..inner.end + start);
    }
}

/// Moves, trims, or removes Slots for a text edit: `edit` (relative to the text that holds
/// `slots`) was replaced by `inserted` bytes. `writer_edit`: the writer changed the text,
/// so an AI Variant that holds the edit becomes Mixed.
/// Slot `except` is left as it is. A Slot inside the removed text ends.
/// The Ghost ranges in `ghosts` and nested in the Slots move with the edit too.
pub(crate) fn shift(
    slots: &mut Vec<SlotNode>,
    ghosts: &mut Vec<Range<usize>>,
    edit: &Range<usize>,
    inserted: usize,
    writer_edit: bool,
    except: Option<SlotId>,
) {
    shift_ghosts(ghosts, edit, inserted);
    slots
        .retain(|node| Some(node.id) == except || relation(&node.range, edit) != Relation::Covered);
    for node in slots.iter_mut().filter(|node| Some(node.id) != except) {
        node.follow(edit, inserted, writer_edit, except);
    }
}

/// Moves, trims, or removes Ghost ranges for a text edit, as [`shift`] does for Slots.
/// A Ghost range left empty ends.
fn shift_ghosts(ghosts: &mut Vec<Range<usize>>, edit: &Range<usize>, inserted: usize) {
    let removed = edit.len();
    for ghost in ghosts.iter_mut() {
        *ghost = match relation(ghost, edit) {
            Relation::Before => ghost.start + inserted - removed..ghost.end + inserted - removed,
            Relation::After => ghost.clone(),
            Relation::Within => ghost.start..ghost.end + inserted - removed,
            Relation::PartialLeft => edit.start + inserted..ghost.end + inserted - removed,
            Relation::PartialRight => ghost.start..edit.start,
            Relation::Covered => 0..0,
        };
    }
    ghosts.retain(|ghost| !ghost.is_empty());
}

/// Where a text edit lies, seen from a Slot range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Relation {
    /// Ends at or before the Slot start (an insert at the start goes before the Slot).
    Before,
    /// Starts at or after the Slot end (an insert at the end goes after the Slot).
    After,
    Within,
    /// Removes the start of the Slot text.
    PartialLeft,
    /// Removes the end of the Slot text.
    PartialRight,
    /// Removes all the Slot text and more, or an empty Slot inside it.
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

/// Sets each visible Active variant text to its range of `text`.
pub(crate) fn sync(slots: &mut [SlotNode], text: &str, base: usize) {
    for node in slots {
        let start = base + node.range.start;
        let active_text = text.get(start..base + node.range.end).map(str::to_owned);
        let active = node.active;
        if let (Some(active_text), Some(variant)) = (active_text, node.variants.get_mut(active)) {
            variant.text = active_text;
        }
        if let Ok(active) = node.active_mut() {
            sync(&mut active.slots, text, start);
        }
    }
}

fn offset_back(range: &Range<usize>, by: usize) -> Range<usize> {
    range.start - by..range.end - by
}

/// `inner` lies in `outer`. An empty `inner` on an edge of `outer` lies outside it.
fn inside(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    let on_edge = inner.is_empty() && (inner.start == outer.start || inner.start == outer.end);
    outer.start <= inner.start && inner.end <= outer.end && !on_edge
}

pub(crate) fn disjoint(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.end <= b.start || b.end <= a.start
}

fn overlaps_partly(a: &Range<usize>, b: &Range<usize>) -> bool {
    !disjoint(a, b) && !inside(a, b) && !inside(b, a)
}
