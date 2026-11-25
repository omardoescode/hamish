//! Ka-teb app: thin GPUI views over the core Document API.

#[expect(
    clippy::disallowed_types,
    reason = "gpui's Action derive builds actions from key binding JSON"
)]
mod actions;
mod ai;
mod context_menu;
mod edit_history;
mod editor;
mod lab;
mod save_open;
mod theme;
mod workspace;

use std::borrow::Cow;

use gpui::{App, KeyBinding};

pub use actions::{
    AiAlternatives, Backspace, CloseMenu, Copy, Cut, Delete, MoveDown, MoveLeft, MoveRight,
    MoveToLineEnd, MoveToLineStart, MoveUp, Newline, Open, OpenAlternatives, Paste, Redo, Save,
    SaveAs, SelectAll, SelectDown, SelectLeft, SelectRight, SelectToLineEnd, SelectToLineStart,
    SelectUp, Stash, ToggleChrome, ToggleGhost, Undo,
};
pub use ai::Ai;
pub use context_menu::ContextMenuItem;
pub use editor::{Editor, Marker, MarkerKind};
pub use lab::{CloseLab, LabRow, LabStatus, RunLabCheck, ToggleLab};
pub use save_open::Notice;
pub use theme::Theme;
pub use workspace::Workspace;

/// Loads the bundled body font, sets the theme, and binds the keys.
///
/// # Errors
/// When the bundled font does not load.
pub fn init(cx: &mut App) -> gpui::Result<()> {
    cx.text_system().add_fonts(vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf")),
    ])?;
    cx.set_global(Theme::default());
    let editor = Some("Editor");
    cx.bind_keys([
        KeyBinding::new("left", MoveLeft, editor),
        KeyBinding::new("right", MoveRight, editor),
        KeyBinding::new("up", MoveUp, editor),
        KeyBinding::new("down", MoveDown, editor),
        KeyBinding::new("home", MoveToLineStart, editor),
        KeyBinding::new("end", MoveToLineEnd, editor),
        KeyBinding::new("shift-left", SelectLeft, editor),
        KeyBinding::new("shift-right", SelectRight, editor),
        KeyBinding::new("shift-up", SelectUp, editor),
        KeyBinding::new("shift-down", SelectDown, editor),
        KeyBinding::new("shift-home", SelectToLineStart, editor),
        KeyBinding::new("shift-end", SelectToLineEnd, editor),
        KeyBinding::new("ctrl-a", SelectAll, editor),
        KeyBinding::new("backspace", Backspace, editor),
        KeyBinding::new("delete", Delete, editor),
        KeyBinding::new("enter", Newline, editor),
        KeyBinding::new("ctrl-c", Copy, editor),
        KeyBinding::new("ctrl-x", Cut, editor),
        KeyBinding::new("ctrl-v", Paste, editor),
        KeyBinding::new("ctrl-z", Undo, editor),
        KeyBinding::new("ctrl-shift-z", Redo, editor),
        KeyBinding::new("ctrl-shift-a", OpenAlternatives, editor),
        KeyBinding::new("ctrl-shift-g", AiAlternatives, editor),
        KeyBinding::new("ctrl-/", ToggleGhost, editor),
        KeyBinding::new("ctrl-shift-x", Stash, editor),
        KeyBinding::new("escape", CloseMenu, editor),
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-shift-s", SaveAs, None),
        KeyBinding::new("ctrl-o", Open, None),
        KeyBinding::new("escape", CloseLab, Some("Lab")),
    ]);
    Ok(())
}
