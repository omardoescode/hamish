//! Vim mode: the Normal, Insert and Visual modes of the Editor as a key state machine over
//! plain text. Raw keys go in; commands for the caret, the text and the register come out
//! (ADR 0008).

mod command_line;
mod hamish;
mod hamishrc;
mod key_feed;
mod keys;
mod motion;
mod normal;
mod operator;
mod panel;
mod put;
mod repeat;
mod text_object;
mod visual;

use std::ops::Range;

use crate::caret::{line_start, previous_boundary};
use crate::{Caret, CaretEdit, Cycle, Direction, Document, EditKind, TrimProposal};
pub use command_line::{
    CommandLine, DirectoryEntry, LineCommand, LineError, LineKey, LineStep, resolve_path,
};
pub use hamish::VimAction;
pub use hamishrc::{
    HAMISHRC_EXAMPLE, Hamishrc, HamishrcError, HamishrcProblem, MapMode, MapTarget,
};
use key_feed::KeyFeed;
pub use key_feed::{Fed, MAX_MAP_DEPTH};
pub use keys::{VIM_KEYS, vim_keys};
use motion::normal_offset;
use operator::Operator;
pub use panel::{AlternativesCommand, AlternativesKeys, LabCommand, RowCommand, SettingsCommand};
pub use put::Put;
use repeat::LastChange;

/// The mode of the Editor while Vim mode is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VimMode {
    /// Keys move the caret and start commands. No key types text.
    #[default]
    Normal,
    /// Keys type text, as with Vim mode off. Escape goes back to Normal.
    Insert,
    /// Motions grow the selection from the char where it started; operators act on it.
    Visual,
    /// As Visual, but the selection is whole lines.
    VisualLine,
}

impl VimMode {
    /// The name the Chrome bar shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Visual => "VISUAL",
            Self::VisualLine => "VISUAL LINE",
        }
    }
}

/// One key press as Vim mode reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimKey {
    /// A key that types this char with no Ctrl, Alt or platform key. Shift is in the char.
    Char(char),
    /// Ctrl with the key of this char, and no Alt or platform key.
    Ctrl(char),
    Escape,
    Enter,
    Backspace,
    /// Any other key or key combination.
    Other,
}

/// What the Editor does for one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VimCommand {
    /// Vim mode does not use the key: the text input takes it as usual.
    PassOn,
    /// Vim mode uses the key, and the text and the caret stay. Examples: a count digit,
    /// the first key of `gg`, a key with no command.
    Nothing,
    /// Show this caret.
    Select(Caret),
    /// Make this edit.
    Edit(CaretEdit),
    /// Make this edit, and put `removed`, the text it removes, in the register (the
    /// system clipboard). An empty `removed` leaves the register as it is.
    Cut { edit: CaretEdit, removed: String },
    /// Put `text` in the register and show `caret`.
    Yank { text: String, caret: Caret },
    /// Put the register text in the Document: the Editor reads the register and runs the
    /// command of [`Put::command`].
    Put(Put),
    /// Undo this many Undo steps.
    Undo(usize),
    /// Redo this many Undo steps.
    Redo(usize),
    /// Ghost this range as one Undo step, and show the caret at its start.
    Ghost(Range<usize>),
    /// Revive each Ghost range this range touches as one Undo step, and show the caret at
    /// its start.
    Revive(Range<usize>),
    /// Stash this range as one Undo step, and show the caret at its start.
    Stash(Range<usize>),
    /// Cycle the Active variant of the Slot at the caret `times` times, as one Undo step.
    /// The caret in no Slot does nothing.
    CycleSlot { cycle: Cycle, times: usize },
    /// Run this action, as its key binding or menu item does.
    Run(VimAction),
}

/// The text Vim mode reads: the Document text, with the spans of its Slots and Ghost ranges
/// for the text objects `iv`, `av`, `ig` and `ag`, its Lab marks, and the open cuts of its Trim
/// proposal. Plain text has none of them.
#[derive(Debug, Clone, Default)]
pub struct VimText<'a> {
    text: &'a str,
    slots: Vec<Range<usize>>,
    ghosts: Vec<Range<usize>>,
    lab_marks: Vec<Range<usize>>,
    /// The cuts still in the Trim proposal; `None` with no proposal open.
    trim_cuts: Option<Vec<Range<usize>>>,
}

impl<'a> From<&'a str> for VimText<'a> {
    fn from(text: &'a str) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }
}

impl<'a> From<&'a Document> for VimText<'a> {
    fn from(document: &'a Document) -> Self {
        Self {
            text: document.text(),
            slots: document
                .slots()
                .into_iter()
                .map(|slot| slot.range)
                .collect(),
            ghosts: document.ghost_ranges(),
            lab_marks: document
                .lab_marks()
                .iter()
                .map(|mark| mark.range.clone())
                .collect(),
            trim_cuts: document.trim_proposal().map(TrimProposal::open_ranges),
        }
    }
}

/// The Vim key state of one Editor: its mode and the keys of a command not yet complete.
#[derive(Debug, Clone, Default)]
pub struct Vim {
    mode: VimMode,
    /// The count typed last: before the operator, or after it.
    count: Option<usize>,
    /// The operator of a command not yet complete, with the count typed before it.
    operator: Option<(Operator, Option<usize>)>,
    /// The first key of a two-key command.
    prefix: Option<Prefix>,
    /// The keys of the command not yet complete.
    typed: Vec<VimKey>,
    /// The column that `j` and `k` keep, in graphemes from the line start. `usize::MAX`
    /// after `$` keeps the line end.
    column: Option<usize>,
    /// The caret offset right after a `j` typed in Insert mode, while a `k` there
    /// can still make `jk` act as Escape.
    after_j: Option<usize>,
    /// The char where the Visual selection started.
    anchor: usize,
    /// The char that Visual mode motions move.
    head: usize,
    /// The change that `.` repeats.
    last: Option<LastChange>,
    /// Where the text typed in Insert mode starts, while `.` records it.
    insert_start: Option<usize>,
    /// `.` runs the keys of the last change: they do not record a new one.
    replaying: bool,
    /// The mode, anchor char and head char of the last Visual selection, for `gv`.
    last_visual: Option<(VimMode, usize, usize)>,
    /// The key that starts the Hamish keys of Normal mode.
    leader: Leader,
    /// The keys typed and not yet read, for the maps of a hamishrc.
    feed: KeyFeed,
}

/// The leader key; Space by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Leader(char);

impl Default for Leader {
    fn default() -> Self {
        Self(' ')
    }
}

/// The first key of a two-key command, waiting for the second key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prefix {
    /// `g`: `gg`, `gv`, and the operators Ghost, Revive and Stash.
    G,
    /// `i` after an operator or in Visual mode: an inner text object.
    Inner,
    /// `a` after an operator or in Visual mode: a text object with its blanks.
    Around,
    /// The leader (Space by default): the Hamish actions.
    Leader,
    /// `]` (forward) or `[` (back): a jump, or a Slot cycle.
    Bracket(Direction),
    /// `Ctrl+W`: the next key moves the keyboard between the Editor and Overflow.
    Window,
}

impl Prefix {
    /// The prefix that `key` is, with `leader` as the leader; `None` for a key that is no
    /// prefix.
    fn of(key: VimKey, leader: Leader) -> Option<Self> {
        if key == VimKey::Ctrl('w') {
            return Some(Self::Window);
        }
        let typed = char_of(key)?;
        if typed == leader.0 {
            return Some(Self::Leader);
        }
        let prefix = match typed {
            'g' => Self::G,
            'i' => Self::Inner,
            'a' => Self::Around,
            ']' => Self::Bracket(Direction::Forward),
            '[' => Self::Bracket(Direction::Back),
            _ => return None,
        };
        Some(prefix)
    }

    /// The prefix starts a command in Normal mode, with `operator` when an operator waits.
    /// Text objects need an operator; the Hamish keys need none.
    fn in_normal(self, operator: bool) -> bool {
        match self {
            Self::G => true,
            Self::Inner | Self::Around => operator,
            Self::Leader | Self::Bracket(_) | Self::Window => !operator,
        }
    }

    /// The prefix starts a command in Visual mode.
    fn in_visual(self) -> bool {
        match self {
            Self::G | Self::Inner | Self::Around | Self::Bracket(_) => true,
            Self::Leader | Self::Window => false,
        }
    }
}

/// The result of one Normal or Visual mode key.
enum Step {
    /// The command is not complete.
    Pending,
    /// The command is complete.
    Done(VimCommand),
    /// The command is complete, and it is a change that `.` repeats.
    Change(VimCommand),
}

impl Vim {
    #[must_use]
    pub fn mode(&self) -> VimMode {
        self.mode
    }

    /// The mode label, then the keys of the command not yet complete, as the Chrome bar
    /// shows them: "NORMAL", "NORMAL 3g", "NORMAL d2i".
    #[must_use]
    pub fn status(&self) -> String {
        let pending: String = self
            .typed
            .iter()
            .filter_map(|key| char_of(*key))
            .map(|typed| if typed == ' ' { '\u{2423}' } else { typed })
            .collect();
        if pending.is_empty() {
            return self.mode.label().to_owned();
        }
        format!("{} {pending}", self.mode.label())
    }

    /// `caret` as Vim mode shows it in `text`. In Normal mode an empty caret past the
    /// last char of a line goes onto that char; on an empty line it stays.
    #[must_use]
    pub fn settle(&self, text: &str, caret: Caret) -> Caret {
        if self.mode != VimMode::Normal || !caret.selection().is_empty() {
            return caret;
        }
        Caret::at(normal_offset(text, caret.clamped(text).offset()))
    }

    /// A click at `offset` of `text` in Visual mode: the head char goes there. With
    /// `extend`, the end of the selection nearer to `offset` goes there. Gives the new
    /// selection; `None` outside Visual mode.
    pub fn click(&mut self, text: &str, offset: usize, extend: bool) -> Option<Caret> {
        if !matches!(self.mode, VimMode::Visual | VimMode::VisualLine) {
            return None;
        }
        let offset = normal_offset(text, offset.min(text.len()));
        if extend && offset.abs_diff(self.anchor) < offset.abs_diff(self.head) {
            self.anchor = self.head;
        }
        self.head = offset;
        self.column = None;
        Some(self.visual_caret(text))
    }

    /// Reads `key` with `caret` in `text` and gives the command for it. In Normal mode
    /// a caret on a line end acts from the last char of the line. In Visual mode the keys
    /// act from the char that Visual mode motions moved last.
    pub fn key<'a>(
        &mut self,
        key: VimKey,
        text: impl Into<VimText<'a>>,
        caret: &Caret,
    ) -> VimCommand {
        let text = text.into();
        let offset = caret.clamped(text.text).offset();
        match self.mode {
            VimMode::Insert => self.insert_key(key, text.text, offset),
            VimMode::Normal => self.normal_key(key, &text, normal_offset(text.text, offset)),
            VimMode::Visual | VimMode::VisualLine => {
                let head = normal_offset(text.text, self.head.min(text.text.len()));
                self.visual_key(key, &text, head)
            }
        }
    }

    /// `jk` typed in a row acts as Escape and removes the `j`.
    fn insert_key(&mut self, key: VimKey, text: &str, offset: usize) -> VimCommand {
        let after_j = self.after_j.take();
        if key == VimKey::Escape {
            self.end_insert(text, offset);
            return VimCommand::Select(Caret::at(step_back(text, offset)));
        }
        match char_of(key) {
            Some('k')
                if after_j == Some(offset)
                    && text
                        .get(..offset)
                        .is_some_and(|before| before.ends_with('j')) =>
            {
                let j = offset - 'j'.len_utf8();
                self.end_insert(text, j);
                let mut edit = CaretEdit::new(j..offset, "", EditKind::Deleting);
                edit.caret = Caret::at(step_back(text, j));
                VimCommand::Edit(edit)
            }
            Some('j') => {
                self.after_j = Some(offset + 'j'.len_utf8());
                VimCommand::PassOn
            }
            Some(_) | None => VimCommand::PassOn,
        }
    }

    /// Back to Normal mode; the text typed from where Insert mode started to `end` goes
    /// into the change that `.` repeats.
    fn end_insert(&mut self, text: &str, end: usize) {
        self.mode = VimMode::Normal;
        let Some(start) = self.insert_start.take() else {
            return;
        };
        if let Some(last) = self.last.as_mut() {
            text.get(start..end)
                .unwrap_or_default()
                .clone_into(&mut last.inserted);
        }
    }

    /// Clears the keys of a complete command, and records a change for `.`.
    fn finish(&mut self, step: Step) -> VimCommand {
        let (command, change) = match step {
            Step::Pending => return VimCommand::Nothing,
            Step::Done(command) => (command, false),
            Step::Change(command) => (command, true),
        };
        let typed = std::mem::take(&mut self.typed);
        self.count = None;
        self.operator = None;
        self.prefix = None;
        if change && !self.replaying {
            self.record(typed, &command);
        }
        command
    }

    /// Adds `key` to the count when it is a count digit.
    fn take_digit(&mut self, key: VimKey) -> bool {
        let Some(digit) = count_digit(key, self.count.is_some()) else {
            return false;
        };
        let count = self.count.unwrap_or(0);
        self.count = Some(count.saturating_mul(10).saturating_add(digit));
        true
    }

    /// The count of the command: the count before the operator times the count after it.
    fn take_count(&mut self) -> Option<usize> {
        let before = self.operator.and_then(|(_, count)| count);
        match (before, self.count.take()) {
            (Some(before), Some(after)) => Some(before.saturating_mul(after)),
            (before, after) => before.or(after),
        }
    }
}

/// The char of `typed` when it is exactly one char.
#[must_use]
pub fn one_char(typed: &str) -> Option<char> {
    let mut chars = typed.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

fn char_of(key: VimKey) -> Option<char> {
    if let VimKey::Char(typed) = key {
        return Some(typed);
    }
    None
}

/// The value of `key` as a count digit. A `0` is a digit only after another digit.
fn count_digit(key: VimKey, after_digit: bool) -> Option<usize> {
    let VimKey::Char(digit @ '0'..='9') = key else {
        return None;
    };
    if digit == '0' && !after_digit {
        return None;
    }
    digit
        .to_digit(10)
        .and_then(|value| usize::try_from(value).ok())
}

/// The caret after Escape at `offset`: one grapheme back, but not past the line start.
fn step_back(text: &str, offset: usize) -> usize {
    if offset > line_start(text, offset) {
        previous_boundary(text, offset)
    } else {
        offset
    }
}

fn select(offset: usize) -> VimCommand {
    VimCommand::Select(Caret::at(offset))
}
