use std::fmt;
use std::ops::Range;

use crate::RangeError;

/// The granularity of a Slot. Word also covers short phrases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Word,
    Sentence,
    Paragraph,
}

/// Who wrote a Variant. Mixed: an AI Variant the writer edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub struct SlotId(u64);

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
    text: String,
    provenance: Provenance,
    slots: Vec<SlotNode>,
}

impl Variant {
    pub(crate) fn new(text: &str, provenance: Provenance) -> Self {
        Self {
            text: text.to_owned(),
            provenance,
            slots: Vec::new(),
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
    /// The range partly overlaps another Slot (strict nesting, ADR 0006).
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
            Self::PartialOverlap => f.write_str("the range partly overlaps another Slot"),
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

    /// Moves or trims this Slot for a text edit at `edit` (relative to the text that holds it).
    fn follow(
        &mut self,
        edit: &Range<usize>,
        inserted: usize,
        changed: bool,
        except: Option<SlotId>,
    ) {
        let removed = edit.len();
        match relation(&self.range, edit) {
            Relation::Before => {
                self.range =
                    self.range.start + inserted - removed..self.range.end + inserted - removed;
            }
            Relation::After | Relation::Covered => {}
            Relation::Within => self.take_within(edit, inserted, changed, except),
            Relation::PartialLeft => self.lose_left(edit, inserted),
            Relation::PartialRight => self.lose_right(edit),
        }
    }

    fn take_within(
        &mut self,
        edit: &Range<usize>,
        inserted: usize,
        changed: bool,
        except: Option<SlotId>,
    ) {
        let local = offset_back(edit, self.range.start);
        if let Ok(children) = self.children_mut() {
            shift(children, &local, inserted, changed, except);
        }
        self.range.end = self.range.end + inserted - edit.len();
        self.mark_edited(changed);
    }

    fn lose_left(&mut self, edit: &Range<usize>, inserted: usize) {
        let lost = 0..edit.end - self.range.start;
        if let Ok(children) = self.children_mut() {
            shift(children, &lost, 0, true, None);
        }
        self.range = edit.start + inserted..self.range.end + inserted - edit.len();
        self.mark_edited(true);
    }

    fn lose_right(&mut self, edit: &Range<usize>) {
        let lost = edit.start - self.range.start..self.range.end - self.range.start;
        if let Ok(children) = self.children_mut() {
            shift(children, &lost, 0, true, None);
        }
        self.range.end = edit.start;
        self.mark_edited(true);
    }

    /// A writer edit to the text of an AI Variant makes it Mixed.
    fn mark_edited(&mut self, changed: bool) {
        let active = self.active;
        let Some(variant) = self.variants.get_mut(active) else {
            return;
        };
        if changed && variant.provenance == Provenance::Ai {
            variant.provenance = Provenance::Mixed;
        }
    }

    fn children(&self) -> &[SlotNode] {
        self.variants
            .get(self.active)
            .map_or(&[], |variant| variant.slots.as_slice())
    }

    fn children_mut(&mut self) -> Result<&mut Vec<SlotNode>, SlotError> {
        let error = SlotError::NoVariant {
            slot: self.id,
            index: self.active,
        };
        self.variants
            .get_mut(self.active)
            .map(|variant| &mut variant.slots)
            .ok_or(error)
    }
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

/// Puts `node` (range relative to the text that holds `slots`) into the tree.
pub(crate) fn place(slots: &mut Vec<SlotNode>, mut node: SlotNode) -> Result<(), SlotError> {
    if let Some(parent) = slots
        .iter_mut()
        .find(|slot| inside(&node.range, &slot.range))
    {
        node.range = offset_back(&node.range, parent.range.start);
        return place(parent.children_mut()?, node);
    }
    if slots
        .iter()
        .any(|slot| overlaps_partly(&slot.range, &node.range))
    {
        return Err(SlotError::PartialOverlap);
    }
    let (mut inner, outer): (Vec<SlotNode>, Vec<SlotNode>) = std::mem::take(slots)
        .into_iter()
        .partition(|slot| inside(&slot.range, &node.range));
    for child in &mut inner {
        child.range = offset_back(&child.range, node.range.start);
    }
    *node.children_mut()? = inner;
    *slots = outer;
    slots.push(node);
    slots.sort_by_key(|slot| slot.range.start);
    Ok(())
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
            .children_mut()
            .ok()
            .and_then(|children| find_mut(children, id, start));
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Moves, trims, or removes Slots for a text edit: `edit` (relative to the text that holds
/// `slots`) was replaced by `inserted` bytes. `changed`: the edit changed the text.
/// Slot `except` is left as it is. A Slot inside the removed text ends.
pub(crate) fn shift(
    slots: &mut Vec<SlotNode>,
    edit: &Range<usize>,
    inserted: usize,
    changed: bool,
    except: Option<SlotId>,
) {
    slots
        .retain(|node| Some(node.id) == except || relation(&node.range, edit) != Relation::Covered);
    for node in slots.iter_mut().filter(|node| Some(node.id) != except) {
        node.follow(edit, inserted, changed, except);
    }
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
        if let Ok(children) = node.children_mut() {
            sync(children, text, start);
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

fn disjoint(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.end <= b.start || b.end <= a.start
}

fn overlaps_partly(a: &Range<usize>, b: &Range<usize>) -> bool {
    !disjoint(a, b) && !inside(a, b) && !inside(b, a)
}
