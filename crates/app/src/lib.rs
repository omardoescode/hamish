//! Hamish app: thin GPUI views over the core Document API.

#[expect(
    clippy::disallowed_types,
    reason = "gpui's Action derive builds actions from key binding JSON"
)]
mod actions;
mod ai;
mod alternatives_panel;
mod caret;
mod chrome;
mod context_menu;
mod editor;
mod effects;
mod fonts;
mod lab;
mod loading;
mod overflow_panel;
mod save_open;
mod settings;
mod settings_modal;
mod shortcuts_sheet;
mod sounds;
mod theme;
mod trim;
mod workspace;

#[cfg(test)]
#[path = "../tests/system_text/mod.rs"]
mod system_text;

use std::borrow::Cow;

use actions::{
    ClosePanel, CloseSelect, CloseShortcuts, ConfirmVariant, DeleteBackward, DeleteQueryBackward,
    DeleteVariant, MoveWordLeft, MoveWordRight, NextRow, PickRow, PreviousRow, SelectNextVariant,
    SelectPreviousVariant, SelectWordLeft, SelectWordRight,
};
use gpui::{App, KeyBinding};

pub use actions::{
    AiAlternatives, Backspace, CloseMenu, CloseSettings, Copy, Cut, Delete, DeleteActiveVariant,
    LeaveOverflow, MoveDown, MoveLeft, MoveRight, MoveToLineEnd, MoveToLineStart, MoveUp,
    NewDocument, Newline, Open, OpenAlternatives, OpenSettings, Paste, Redo, Save, SaveAs,
    SelectAll, SelectDown, SelectLeft, SelectRight, SelectToLineEnd, SelectToLineStart, SelectUp,
    Stash, ToggleGhost, ToggleOverflow, Undo, UseOverflowBlock,
};
pub use ai::{Ai, AiFailure, AiStatus};
pub use alternatives_panel::AlternativesPanel;
pub use context_menu::ContextMenuItem;
pub use editor::{Editor, Marker, MarkerKind};
pub use effects::PlayingEffect;
pub use fonts::{font_names, load_fonts_in_background};
pub use hamish_core::{AiAlternativesError, VariantRow};
pub use lab::{CloseLab, Lab, LabRow, LabStatus, RunLabCheck, ToggleLab};
pub use overflow_panel::OverflowPanel;
pub use save_open::Notice;
pub use settings::use_settings;
pub use settings_modal::SettingsModal;
pub use shortcuts_sheet::{ShortcutGroup, ShortcutRow, shortcut_groups};
pub use sounds::{DeviceSounds, SoundSink, Sounds};
pub use theme::Theme;
pub use trim::{CloseTrim, MakeTrimCuts, RunTrim, TrimBar, WalkCut, WalkKeep, WalkTrim};
pub use workspace::Workspace;

/// Loads the bundled font (Regular, Bold, Italic, Bold Italic), sets the default settings
/// and theme, and binds the keys. [`use_settings`] then puts in the settings from the file.
///
/// # Errors
/// When the bundled font does not load.
pub fn init(cx: &mut App) -> gpui::Result<()> {
    cx.text_system().add_fonts(vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-BoldItalic.ttf")),
    ])?;
    use_settings(
        None,
        hamish_core::LoadedSettings {
            settings: hamish_core::Settings::default(),
            problem: None,
        },
        cx,
    );
    for context in ["Editor", "OverflowPanel"] {
        let context = Some(context);
        cx.bind_keys([
            KeyBinding::new("left", MoveLeft, context),
            KeyBinding::new("right", MoveRight, context),
            KeyBinding::new("ctrl-left", MoveWordLeft, context),
            KeyBinding::new("ctrl-right", MoveWordRight, context),
            KeyBinding::new("up", MoveUp, context),
            KeyBinding::new("down", MoveDown, context),
            KeyBinding::new("home", MoveToLineStart, context),
            KeyBinding::new("end", MoveToLineEnd, context),
            KeyBinding::new("shift-left", SelectLeft, context),
            KeyBinding::new("shift-right", SelectRight, context),
            KeyBinding::new("ctrl-shift-left", SelectWordLeft, context),
            KeyBinding::new("ctrl-shift-right", SelectWordRight, context),
            KeyBinding::new("shift-up", SelectUp, context),
            KeyBinding::new("shift-down", SelectDown, context),
            KeyBinding::new("shift-home", SelectToLineStart, context),
            KeyBinding::new("shift-end", SelectToLineEnd, context),
            KeyBinding::new("ctrl-a", SelectAll, context),
            KeyBinding::new("backspace", Backspace, context),
            KeyBinding::new("delete", Delete, context),
            KeyBinding::new("enter", Newline, context),
            KeyBinding::new("ctrl-c", Copy, context),
            KeyBinding::new("ctrl-x", Cut, context),
            KeyBinding::new("ctrl-v", Paste, context),
            KeyBinding::new("ctrl-z", Undo, context),
            KeyBinding::new("ctrl-shift-z", Redo, context),
        ]);
    }
    let editor = Some("Editor");
    let overflow = Some("OverflowPanel");
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-a", OpenAlternatives, editor),
        KeyBinding::new("ctrl-shift-g", AiAlternatives, editor),
        KeyBinding::new("ctrl-/", ToggleGhost, editor),
        KeyBinding::new("ctrl-shift-x", Stash, editor),
        KeyBinding::new("escape", CloseMenu, editor),
        KeyBinding::new("ctrl-shift-backspace", DeleteActiveVariant, editor),
        KeyBinding::new("ctrl-enter", UseOverflowBlock, overflow),
        KeyBinding::new("escape", LeaveOverflow, overflow),
        KeyBinding::new("ctrl-shift-o", ToggleOverflow, Some("Workspace")),
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-shift-s", SaveAs, None),
        KeyBinding::new("ctrl-o", Open, None),
        KeyBinding::new("ctrl-n", NewDocument, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
        KeyBinding::new("escape", CloseSettings, Some("SettingsModal")),
        KeyBinding::new("escape", CloseShortcuts, Some("ShortcutsSheet")),
        KeyBinding::new("escape", CloseLab, Some("Lab")),
    ]);
    // In Normal and Visual mode these keys go to Vim mode, not to their text edits.
    let vim_normal = Some("Editor && (VimNormal || VimVisual)");
    cx.bind_keys(
        ["backspace", "delete", "enter", "ctrl-x", "ctrl-v"]
            .map(|key| KeyBinding::new(key, gpui::NoAction {}, vim_normal)),
    );
    for context in ["AlternativesPanel", "Lab"] {
        let context = Some(context);
        cx.bind_keys([
            KeyBinding::new("ctrl-z", Undo, context),
            KeyBinding::new("ctrl-shift-z", Redo, context),
        ]);
    }
    let panel = Some("AlternativesPanel");
    cx.bind_keys([
        KeyBinding::new("up", SelectPreviousVariant, panel),
        KeyBinding::new("down", SelectNextVariant, panel),
        KeyBinding::new("enter", ConfirmVariant, panel),
        KeyBinding::new("escape", ClosePanel, panel),
        KeyBinding::new("backspace", DeleteBackward, panel),
        KeyBinding::new("ctrl-backspace", DeleteVariant, panel),
        KeyBinding::new("ctrl-shift-g", AiAlternatives, panel),
        KeyBinding::new("ctrl-v", Paste, panel),
    ]);
    let select = Some("SettingsSelect");
    cx.bind_keys([
        KeyBinding::new("up", PreviousRow, select),
        KeyBinding::new("down", NextRow, select),
        KeyBinding::new("enter", PickRow, select),
        KeyBinding::new("escape", CloseSelect, select),
        KeyBinding::new("backspace", DeleteQueryBackward, select),
    ]);
    Ok(())
}
