//! Ka-teb core: the Document editing API. No GPUI.

mod ai;
mod alternatives_list;
mod around;
mod article;
mod caret;
mod counter;
mod document;
mod edit_history;
mod edit_session;
mod focus_zoom;
mod ghost;
mod lab;
mod markdown;
mod motion;
mod overflow;
mod particles;
mod settings;
mod slot;
mod sound;
mod storage;
mod theme;
mod title;
mod tree;
mod trim;
mod undo;

pub use ai::{AiAdapter, AiError, AiRequest, AiTrigger, ApplyError, CancelToken, ClaudeBackend};
pub use alternatives_list::{
    AiAlternativesError, AlternativesList, ListEditor, ListOutcome, ListTarget, QUICK_FILL,
    VariantRow,
};
pub use around::{paragraph_around, sentence_around};
pub use caret::{Caret, CaretEdit, Motion, byte_range, follow_change, utf16_range, word_at};
pub use counter::Counter;
pub use document::{Document, RangeError};
pub use edit_history::EditKind;
pub(crate) use edit_history::{EditHistory, EditedText};
pub use edit_session::{EditSession, SlotChanges};
pub use focus_zoom::{FOCUS_ZOOM_SCALE, FocusZoom, paragraph_index, paragraph_range};
pub use ghost::GhostError;
pub use lab::{LabCheck, LabError, LabFinding, LabMark, LabMarkId, LabRequest};
pub use markdown::{MarkdownSpan, MarkdownStyle};
pub use motion::Effect;
pub use overflow::{Overflow, OverflowError};
pub use particles::{Particle, ParticleArea, ParticleLook};
pub use settings::{LoadedSettings, Setting, Settings, SettingsError, Toggle, settings_path};
pub use slot::{Cycle, Level, Provenance, Slot, SlotError, SlotId, Switch, Variant};
pub use sound::Sound;
pub use storage::{Dropped, Opened, StorageError, sidecar_path};
pub use theme::{Palette, ThemeName, contrast_ratio};
pub use title::document_title;
pub use trim::{TrimCut, TrimError, TrimLevel, TrimProposal, TrimRequest};
