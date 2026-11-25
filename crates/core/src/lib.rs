//! Ka-teb core: the Document editing API. No GPUI.

mod ai;
mod counter;
mod document;
mod slot;

pub use ai::{AiAdapter, AiError, AiRequest, ApplyError};
pub use counter::Counter;
pub use document::{Document, RangeError};
pub use slot::{Cycle, Level, Provenance, Slot, SlotError, SlotId, Variant};
