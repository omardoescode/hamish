//! Vim mode keys in the panels: the rows of the Alternatives panel, the Lab popover and
//! Settings. Panel rows take no operators and no motions.

use super::VimKey;
use crate::Level;

/// A panel that reads Vim mode keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimPanel {
    /// The rows of the Alternatives panel. `i` types in the selected row.
    Alternatives,
    /// The Lab popover: one row per Lab check.
    Lab,
    /// The fields of Settings.
    Settings,
    /// The rows of an open Settings select. Keys other than `j` and `k` type the search.
    SettingsSelect,
}

/// What a panel does for one Vim mode key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelCommand {
    /// The panel does not use the key: its key bindings or text input take it as usual.
    PassOn,
    /// The panel uses the key, and nothing changes.
    Nothing,
    /// The next row or field.
    Down,
    /// The row or field before.
    Up,
    /// A lower value of the field.
    Left,
    /// A higher value of the field.
    Right,
    /// Act on the row or field: make the Variant active, run the Lab check, open the select.
    Enter,
    /// Close the panel, the popover or the select.
    Close,
    /// Start to type in the selected row.
    Insert,
    /// Stop to type in the row; the keys move between rows again.
    EndInsert,
    /// Delete the Variant in the selected row.
    DeleteRow,
    /// Show the target at this Level.
    ChooseLevel(Level),
}

/// The Vim key state of one panel: typing in a row, and the first `d` of `dd`.
#[derive(Debug, Clone, Default)]
pub struct PanelKeys {
    inserting: bool,
    after_d: bool,
}

impl PanelKeys {
    /// Keys type in the selected row of the Alternatives panel.
    #[must_use]
    pub fn inserting(&self) -> bool {
        self.inserting
    }

    /// Reads `key` in `panel` and gives the command for it.
    pub fn key(&mut self, panel: VimPanel, key: VimKey) -> PanelCommand {
        if self.inserting {
            self.inserting = key != VimKey::Escape;
            return insert_key(key);
        }
        if std::mem::take(&mut self.after_d) {
            return after_d(key);
        }
        let typed = match key {
            VimKey::Char(typed) => typed,
            VimKey::Enter => return PanelCommand::Enter,
            VimKey::Escape => return PanelCommand::Close,
            VimKey::Backspace if panel == VimPanel::Alternatives => return PanelCommand::Nothing,
            VimKey::Backspace | VimKey::Ctrl(_) | VimKey::Other => return PanelCommand::PassOn,
        };
        match (panel, typed) {
            (_, 'j') => PanelCommand::Down,
            (_, 'k') => PanelCommand::Up,
            (VimPanel::SettingsSelect, _) => PanelCommand::PassOn,
            (VimPanel::Settings, 'h') => PanelCommand::Left,
            (VimPanel::Settings, 'l') => PanelCommand::Right,
            (VimPanel::Alternatives, typed) => self.alternatives_key(typed),
            (VimPanel::Lab | VimPanel::Settings, _) => PanelCommand::Nothing,
        }
    }

    fn alternatives_key(&mut self, typed: char) -> PanelCommand {
        match typed {
            'i' => {
                self.inserting = true;
                PanelCommand::Insert
            }
            'd' => {
                self.after_d = true;
                PanelCommand::Nothing
            }
            '1' => PanelCommand::ChooseLevel(Level::Word),
            '2' => PanelCommand::ChooseLevel(Level::Sentence),
            '3' => PanelCommand::ChooseLevel(Level::Paragraph),
            _ => PanelCommand::Nothing,
        }
    }
}

/// A key while typing in a row: Escape stops; the text input takes the others.
fn insert_key(key: VimKey) -> PanelCommand {
    match key {
        VimKey::Escape => PanelCommand::EndInsert,
        VimKey::Char(_) | VimKey::Ctrl(_) | VimKey::Enter | VimKey::Backspace | VimKey::Other => {
            PanelCommand::PassOn
        }
    }
}

/// The key after the first `d`: a second `d` deletes the Variant.
fn after_d(key: VimKey) -> PanelCommand {
    match key {
        VimKey::Char('d') => PanelCommand::DeleteRow,
        VimKey::Char(_)
        | VimKey::Ctrl(_)
        | VimKey::Escape
        | VimKey::Enter
        | VimKey::Backspace
        | VimKey::Other => PanelCommand::Nothing,
    }
}
