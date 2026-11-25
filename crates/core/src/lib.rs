//! Ka-teb core: the Document editing API. No GPUI.

mod counter;
mod document;

pub use counter::Counter;
pub use document::{Document, RangeError};
