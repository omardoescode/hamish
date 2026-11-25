//! Ka-teb core: the Document editing API. No GPUI.

mod counter;
mod document;
mod slot;

pub use counter::Counter;
pub use document::{Document, RangeError};
pub use slot::{Level, Provenance, Slot, SlotError, SlotId, Variant};
