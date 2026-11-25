//! Ka-teb app: thin GPUI views over the core Document API.

#[expect(
    clippy::disallowed_types,
    reason = "gpui's Action derive builds actions from key binding JSON"
)]
mod actions;
mod ai;
mod alternatives_panel;
mod context_menu;
mod edit_history;
mod editor;
mod lab;
mod overflow_panel;
mod save_open;
mod theme;
mod workspace;

use std::borrow::Cow;

use actions::{
    ClosePanel, ConfirmVariant, DeleteBackward, DeleteVariant, SelectNextVariant,
    SelectPreviousVariant,
};
use gpui::{App, KeyBinding};

pub use actions::{
    AiAlternatives, Backspace, CloseMenu, Copy, Cut, Delete, LeaveOverflow, MoveDown, MoveLeft,
    MoveRight, MoveToLineEnd, MoveToLineStart, MoveUp, Newline, Open, OpenAlternatives, Paste,
    Redo, Save, SaveAs, SelectAll, SelectDown, SelectLeft, SelectRight, SelectToLineEnd,
    SelectToLineStart, SelectUp, Stash, ToggleChrome, ToggleGhost, ToggleOverflow, Undo,
    UseOverflowBlock,
};
pub use ai::Ai;
pub use alternatives_panel::{AlternativesPanel, VariantRow};
pub use context_menu::ContextMenuItem;
pub use editor::{Editor, Marker, MarkerKind};
pub use lab::{CloseLab, LabRow, LabStatus, RunLabCheck, ToggleLab};
pub use overflow_panel::OverflowPanel;
pub use save_open::Notice;
pub use theme::Theme;
pub use workspace::Workspace;

/// Loads the bundled body font (Regular, Bold, Italic, Bold Italic), sets the theme, and binds the keys.
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
    cx.set_global(Theme::default());
    for context in ["Editor", "OverflowPanel"] {
        let context = Some(context);
        cx.bind_keys([
            KeyBinding::new("left", MoveLeft, context),
            KeyBinding::new("right", MoveRight, context),
            KeyBinding::new("up", MoveUp, context),
            KeyBinding::new("down", MoveDown, context),
            KeyBinding::new("home", MoveToLineStart, context),
            KeyBinding::new("end", MoveToLineEnd, context),
            KeyBinding::new("shift-left", SelectLeft, context),
            KeyBinding::new("shift-right", SelectRight, context),
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
        KeyBinding::new("ctrl-enter", UseOverflowBlock, overflow),
        KeyBinding::new("escape", LeaveOverflow, overflow),
        KeyBinding::new("ctrl-shift-o", ToggleOverflow, Some("Workspace")),
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-shift-s", SaveAs, None),
        KeyBinding::new("ctrl-o", Open, None),
        KeyBinding::new("escape", CloseLab, Some("Lab")),
    ]);
    let panel = Some("AlternativesPanel");
    cx.bind_keys([
        KeyBinding::new("up", SelectPreviousVariant, panel),
        KeyBinding::new("down", SelectNextVariant, panel),
        KeyBinding::new("enter", ConfirmVariant, panel),
        KeyBinding::new("escape", ClosePanel, panel),
        KeyBinding::new("backspace", DeleteBackward, panel),
        KeyBinding::new("ctrl-backspace", DeleteVariant, panel),
        KeyBinding::new("ctrl-v", Paste, panel),
    ]);
    Ok(())
}
