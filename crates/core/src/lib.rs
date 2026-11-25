//! Ka-teb core: the Document editing API. No GPUI.

mod ai;
mod article;
mod counter;
mod document;
mod ghost;
mod lab;
mod overflow;
mod slot;

pub use ai::{AiAdapter, AiError, AiRequest, ApplyError, ClaudeBackend};
pub use counter::Counter;
pub use document::{Document, RangeError};
pub use ghost::GhostError;
pub use lab::{LabCheck, LabError, LabFinding, LabMark, LabMarkId, LabRequest};
pub use overflow::{Overflow, OverflowError};
pub use slot::{Cycle, Level, Provenance, Slot, SlotError, SlotId, Variant};
