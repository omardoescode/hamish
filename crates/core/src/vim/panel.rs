//! Vim mode keys in the panels: the rows of the Alternatives panel, the Lab popover and
//! Settings. Panel rows take no operators and no motions. Each panel has its own command.

use super::{VimKey, char_of};
use crate::{Direction, Level};

/// What the Vim mode keys do with the rows of any panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowCommand {
    /// The next row or field.
    Down,
    /// The row or field before.
    Up,
    /// Act on the row or field: make the Variant active, run the Lab check, open the select.
    Enter,
    /// Close the panel or the popover.
    Close,
}

impl RowCommand {
    /// `j`, `k`, Enter and Escape; `None` for another key.
    #[must_use]
    pub fn of(key: VimKey) -> Option<Self> {
        match key {
            VimKey::Enter => Some(Self::Enter),
            VimKey::Escape => Some(Self::Close),
            VimKey::Char(_) | VimKey::Ctrl(_) | VimKey::Backspace | VimKey::Other => {
                Self::in_select(key)
            }
        }
    }

    /// The rows of an open Settings select: `j` and `k` only. `None` for another key: it
    /// types the search.
    #[must_use]
    pub fn in_select(key: VimKey) -> Option<Self> {
        match char_of(key)? {
            'j' => Some(Self::Down),
            'k' => Some(Self::Up),
            _ => None,
        }
    }
}

/// What the Lab popover does for one Vim mode key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabCommand {
    /// The Lab does not use the key: its key bindings take it as usual.
    PassOn,
    /// The Lab uses the key, and nothing changes.
    Nothing,
    /// A row key on the Lab checks.
    Row(RowCommand),
}

impl LabCommand {
    #[must_use]
    pub fn of(key: VimKey) -> Self {
        if let Some(row) = RowCommand::of(key) {
            return Self::Row(row);
        }
        if char_of(key).is_some() {
            Self::Nothing
        } else {
            Self::PassOn
        }
    }
}

/// What Settings do for one Vim mode key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCommand {
    /// Settings do not use the key: their key bindings take it as usual.
    PassOn,
    /// Settings use the key, and nothing changes.
    Nothing,
    /// A row key on the fields.
    Row(RowCommand),
    /// `l` (forward) gives a higher value of the field, `h` (back) a lower one.
    Step(Direction),
}

impl SettingsCommand {
    #[must_use]
    pub fn of(key: VimKey) -> Self {
        if let Some(row) = RowCommand::of(key) {
            return Self::Row(row);
        }
        match char_of(key) {
            Some('h') => Self::Step(Direction::Back),
            Some('l') => Self::Step(Direction::Forward),
            Some(_) => Self::Nothing,
            None => Self::PassOn,
        }
    }
}

/// What the Alternatives panel does for one Vim mode key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlternativesCommand {
    /// The panel does not use the key: its key bindings or text input take it as usual.
    PassOn,
    /// The panel uses the key, and nothing changes.
    Nothing,
    /// A row key on the rows.
    Row(RowCommand),
    /// Start to type in the selected row.
    Insert,
    /// Stop to type in the row; the keys move between rows again.
    EndInsert,
    /// Delete the Variant in the selected row.
    DeleteRow,
    /// Show the target at this Level.
    ChooseLevel(Level),
}

/// The Vim key state of the Alternatives panel: typing in a row, and the first `d` of `dd`.
#[derive(Debug, Clone, Default)]
pub struct AlternativesKeys {
    inserting: bool,
    after_d: bool,
}

impl AlternativesKeys {
    /// Keys type in the selected row.
    #[must_use]
    pub fn inserting(&self) -> bool {
        self.inserting
    }

    /// Reads `key` and gives the command for it.
    pub fn key(&mut self, key: VimKey) -> AlternativesCommand {
        if self.inserting {
            self.inserting = key != VimKey::Escape;
            return insert_key(key);
        }
        if std::mem::take(&mut self.after_d) {
            return after_d(key);
        }
        if let Some(row) = RowCommand::of(key) {
            return AlternativesCommand::Row(row);
        }
        if key == VimKey::Backspace {
            return AlternativesCommand::Nothing;
        }
        char_of(key).map_or(AlternativesCommand::PassOn, |typed| self.typed(typed))
    }

    fn typed(&mut self, typed: char) -> AlternativesCommand {
        match typed {
            'i' => {
                self.inserting = true;
                AlternativesCommand::Insert
            }
            'd' => {
                self.after_d = true;
                AlternativesCommand::Nothing
            }
            '1' => AlternativesCommand::ChooseLevel(Level::Word),
            '2' => AlternativesCommand::ChooseLevel(Level::Sentence),
            '3' => AlternativesCommand::ChooseLevel(Level::Paragraph),
            _ => AlternativesCommand::Nothing,
        }
    }
}

/// A key while typing in a row: Escape stops; the text input takes the others.
fn insert_key(key: VimKey) -> AlternativesCommand {
    if key == VimKey::Escape {
        AlternativesCommand::EndInsert
    } else {
        AlternativesCommand::PassOn
    }
}

/// The key after the first `d`: a second `d` deletes the Variant.
fn after_d(key: VimKey) -> AlternativesCommand {
    if key == VimKey::Char('d') {
        AlternativesCommand::DeleteRow
    } else {
        AlternativesCommand::Nothing
    }
}
