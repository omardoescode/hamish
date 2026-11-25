//! Ka-teb core: the Document editing API. No GPUI.

mod ai;
mod article;
mod counter;
mod document;
mod focus_zoom;
mod ghost;
mod lab;
mod markdown;
mod motion;
mod overflow;
mod settings;
mod slot;
mod sound;
mod storage;
mod trim;

pub use ai::{AiAdapter, AiError, AiRequest, ApplyError, ClaudeBackend};
pub use counter::Counter;
pub use document::{Document, RangeError};
pub use focus_zoom::{FOCUS_ZOOM_SCALE, FocusZoom, paragraph_index, paragraph_range};
pub use ghost::GhostError;
pub use lab::{LabCheck, LabError, LabFinding, LabMark, LabMarkId, LabRequest};
pub use markdown::{MarkdownSpan, MarkdownStyle};
pub use motion::Effect;
pub use overflow::{Overflow, OverflowError};
pub use settings::{LoadedSettings, Setting, Settings, SettingsError, ThemeName, settings_path};
pub use slot::{Cycle, Level, Provenance, Slot, SlotError, SlotId, Switch, Variant};
pub use sound::Sound;
pub use storage::{Dropped, Opened, StorageError, sidecar_path};
pub use trim::{TrimCut, TrimError, TrimLevel, TrimProposal, TrimRequest};
