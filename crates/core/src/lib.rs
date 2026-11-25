//! Ka-teb core: the Document editing API. No GPUI.

mod ai;
mod article;
mod counter;
mod document;
mod ghost;
mod lab;
mod markdown;
mod overflow;
mod settings;
mod slot;
mod storage;
mod trim;

pub use ai::{AiAdapter, AiError, AiRequest, ApplyError, ClaudeBackend};
pub use counter::Counter;
pub use document::{Document, RangeError};
pub use ghost::GhostError;
pub use lab::{LabCheck, LabError, LabFinding, LabMark, LabMarkId, LabRequest};
pub use markdown::{MarkdownSpan, MarkdownStyle};
pub use overflow::{Overflow, OverflowError};
pub use settings::{LoadedSettings, Setting, Settings, SettingsError, ThemeName, settings_path};
pub use slot::{Cycle, Level, Provenance, Slot, SlotError, SlotId, Variant};
pub use storage::{Dropped, Opened, StorageError, sidecar_path};
pub use trim::{TrimCut, TrimError, TrimLevel, TrimProposal, TrimRequest};
