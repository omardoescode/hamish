//! Hamish core: the Document editing API. No GPUI.

mod ai;
mod alternatives_list;
mod around;
mod article;
mod caret;
mod counter;
mod direction;
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
mod select_search;
mod settings;
mod shortcut;
mod slot;
mod sound;
mod storage;
mod theme;
mod title;
mod tree;
mod trim;
mod undo;
mod vim;

pub use ai::{AiAdapter, AiError, AiRequest, AiTrigger, ApplyError, CancelToken, ClaudeBackend};
pub use alternatives_list::{
    AiAlternativesError, AlternativesList, ListEditor, ListOutcome, ListTarget, QUICK_FILL,
    VariantRow,
};
pub use around::{paragraph_around, sentence_around};
pub use caret::{Caret, CaretEdit, Motion, byte_range, follow_change, utf16_range, word_at};
pub use counter::Counter;
pub use direction::Direction;
pub use document::{Document, RangeError};
pub use edit_history::EditKind;
pub(crate) use edit_history::{EditHistory, EditedText};
pub use edit_session::{EditSession, SlotChanges};
pub use focus_zoom::{FOCUS_ZOOM_SCALE, FocusZoom, paragraph_index, paragraph_range};
pub use ghost::GhostError;
pub use lab::{LabCheck, LabError, LabFinding, LabMark, LabMarkId, LabRequest};
pub use markdown::{MarkdownSpan, MarkdownStyle};
pub use motion::{Effect, caret_shown, next_caret_toggle};
pub use overflow::{Overflow, OverflowError};
pub use particles::{Particle, ParticleArea, ParticleLook};
pub use select_search::SelectSearch;
pub use settings::{
    LoadedSettings, Setting, Settings, SettingsError, Toggle, hamishrc_path, settings_path,
};
pub use shortcut::key_label;
pub use slot::{Cycle, Level, Provenance, Slot, SlotError, SlotId, Switch, Variant};
pub use sound::Sound;
pub use storage::{Dropped, Opened, StorageError};
pub use theme::{Palette, ThemeName, contrast_ratio};
pub use title::document_title;
pub use trim::{TrimCut, TrimError, TrimLevel, TrimProposal, TrimRequest};
pub use vim::{
    AlternativesCommand, AlternativesKeys, CommandLine, DirectoryEntry, Fed, HAMISHRC_EXAMPLE,
    Hamishrc, HamishrcError, HamishrcProblem, LabCommand, LineCommand, LineError, LineKey,
    LineStep, MAX_MAP_DEPTH, MapMode, MapTarget, Put, RowCommand, SettingsCommand, VIM_KEYS, Vim,
    VimAction, VimCommand, VimKey, VimMode, VimText, one_char, resolve_path, vim_keys,
};
