//! Vim mode: the Normal and Insert modes of the Editor as a key state machine over plain
//! text. Raw keys go in; commands for the caret and the text come out (ADR 0008).

use unicode_segmentation::UnicodeSegmentation;

use crate::caret::{line_start, next_boundary, previous_boundary};
use crate::{Caret, CaretEdit, EditKind};

/// The mode of the Editor while Vim mode is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VimMode {
    /// Keys move the caret and start commands. No key types text.
    #[default]
    Normal,
    /// Keys type text, as with Vim mode off. Escape goes back to Normal.
    Insert,
}

impl VimMode {
    /// The name the Chrome bar shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
        }
    }
}

/// One key press as Vim mode reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimKey {
    /// A key that types this char with no Ctrl, Alt or platform key. Shift is in the char.
    Char(char),
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
}

/// The keys of Vim mode as the Shortcuts sheet lists them: what each does, then its keys.
pub const VIM_KEYS: [(&str, &str); 10] = [
    ("Move left, down, up, right", "h j k l"),
    ("Next word start, word start before, word end", "w b e"),
    ("Line start, line end", "0 $"),
    ("First line, last line (a count: that line)", "g g  G"),
    ("First non-blank of the next line", "Enter"),
    ("A count repeats a motion", "3 w"),
    ("Insert mode before, after the caret", "i a"),
    ("Insert mode on a new line below, above", "o O"),
    ("Back to Normal mode", "Esc"),
    ("Text edits in Insert mode", "Ctrl keys as usual"),
];

/// The Vim key state of one Editor: its mode and the keys of a command not yet complete.
#[derive(Debug, Clone, Default)]
pub struct Vim {
    mode: VimMode,
    count: Option<usize>,
    /// The first key of a two-key command, as the first `g` of `gg`.
    prefix: Option<char>,
    /// The column that `j` and `k` keep, in graphemes from the line start. `usize::MAX`
    /// after `$` keeps the line end.
    column: Option<usize>,
}

/// The kind of a grapheme for word moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Blank,
    Word,
    Punctuation,
}

impl Vim {
    #[must_use]
    pub fn mode(&self) -> VimMode {
        self.mode
    }

    /// The mode label, then the keys of the command not yet complete (the count, then the
    /// first key of a pair), as the Chrome bar shows them: "NORMAL", "NORMAL 3g".
    #[must_use]
    pub fn status(&self) -> String {
        let mut pending = self
            .count
            .map_or_else(String::new, |count| count.to_string());
        pending.extend(self.prefix);
        if pending.is_empty() {
            return self.mode.label().to_owned();
        }
        format!("{} {pending}", self.mode.label())
    }

    /// Reads `key` with `caret` in `text` and gives the command for it. In Normal mode
    /// a caret on a line end acts from the last char of the line.
    pub fn key(&mut self, key: VimKey, text: &str, caret: &Caret) -> VimCommand {
        let offset = caret.clamped(text).offset();
        match self.mode {
            VimMode::Insert => self.insert_key(key, text, offset),
            VimMode::Normal => self.normal_key(key, text, normal_offset(text, offset)),
        }
    }

    fn insert_key(&mut self, key: VimKey, text: &str, offset: usize) -> VimCommand {
        if key != VimKey::Escape {
            return VimCommand::PassOn;
        }
        self.mode = VimMode::Normal;
        let back = if offset > line_start(text, offset) {
            previous_boundary(text, offset)
        } else {
            offset
        };
        VimCommand::Select(Caret::at(back))
    }

    fn normal_key(&mut self, key: VimKey, text: &str, offset: usize) -> VimCommand {
        if let Some(prefix) = self.prefix.take() {
            let count = self.count.take();
            self.column = None;
            return pair(prefix, key, count, text);
        }
        match key {
            VimKey::Char(digit @ '0'..='9') if digit != '0' || self.count.is_some() => {
                let value = usize::try_from(digit.to_digit(10).unwrap_or(0)).unwrap_or(0);
                let count = self.count.unwrap_or(0);
                self.count = Some(count.saturating_mul(10).saturating_add(value));
                VimCommand::Nothing
            }
            VimKey::Escape => {
                self.count = None;
                VimCommand::Nothing
            }
            VimKey::Char('g') => {
                self.prefix = Some('g');
                VimCommand::Nothing
            }
            VimKey::Char(_) | VimKey::Enter | VimKey::Backspace | VimKey::Other => {
                let count = self.count.take();
                self.command(key, count, text, offset)
            }
        }
    }

    fn command(
        &mut self,
        key: VimKey,
        count: Option<usize>,
        text: &str,
        offset: usize,
    ) -> VimCommand {
        let times = count.unwrap_or(1);
        if !matches!(key, VimKey::Char('j' | 'k')) {
            self.column = None;
        }
        match key {
            VimKey::Char('h') | VimKey::Backspace => select(left(text, offset, times)),
            VimKey::Char('l') => select(right(text, offset, times)),
            VimKey::Char('j') => {
                select(self.vertical(text, offset, lines_down(text, offset, times)))
            }
            VimKey::Char('k') => select(self.vertical(text, offset, lines_up(text, offset, times))),
            VimKey::Enter => select(first_non_blank(text, lines_down(text, offset, times))),
            VimKey::Char('w') => select(repeat(text, offset, times, word_start_after)),
            VimKey::Char('b') => select(repeat(text, offset, times, word_start_before)),
            VimKey::Char('e') => select(repeat(text, offset, times, word_end_after)),
            VimKey::Char('0') => select(line_start(text, offset)),
            VimKey::Char('$') => {
                self.column = Some(usize::MAX);
                select(normal_offset(
                    text,
                    line_end(text, lines_down(text, offset, times - 1)),
                ))
            }
            VimKey::Char('G') => {
                let line =
                    count.map_or_else(|| line_start(text, text.len()), |n| line_number(text, n));
                select(first_non_blank(text, line))
            }
            VimKey::Char('i') => self.insert_at(offset),
            VimKey::Char('a') => self.insert_at(append_offset(text, offset)),
            VimKey::Char('o') => self.open_line(text, line_end(text, offset), false),
            VimKey::Char('O') => self.open_line(text, line_start(text, offset), true),
            VimKey::Char(_) | VimKey::Escape | VimKey::Other => VimCommand::Nothing,
        }
    }

    /// The offset in the line at `start` at the column `j` and `k` keep.
    fn vertical(&mut self, text: &str, offset: usize, start: usize) -> usize {
        let column = *self.column.get_or_insert_with(|| column_of(text, offset));
        at_column(text, start, column)
    }

    fn insert_at(&mut self, offset: usize) -> VimCommand {
        self.mode = VimMode::Insert;
        VimCommand::Select(Caret::at(offset))
    }

    /// A new empty line at `at` (a line end, or a line start when `above`), in Insert mode.
    fn open_line(&mut self, text: &str, at: usize, above: bool) -> VimCommand {
        self.mode = VimMode::Insert;
        let at = at.min(text.len());
        let mut edit = CaretEdit::new(at..at, "\n", EditKind::Typing);
        if above {
            edit.caret = Caret::at(at);
        }
        VimCommand::Edit(edit)
    }
}

/// The command of the two keys `prefix` and `key`, with `count`.
fn pair(prefix: char, key: VimKey, count: Option<usize>, text: &str) -> VimCommand {
    match (prefix, key) {
        ('g', VimKey::Char('g')) => {
            select(first_non_blank(text, line_number(text, count.unwrap_or(1))))
        }
        _ => VimCommand::Nothing,
    }
}

fn select(offset: usize) -> VimCommand {
    VimCommand::Select(Caret::at(offset))
}

/// The newline at the end of the line that holds `offset`, else the text end.
fn line_end(text: &str, offset: usize) -> usize {
    text.get(offset..)
        .and_then(|after| after.find('\n'))
        .map_or(text.len(), |newline| offset + newline)
}

/// `offset` on the last char of its line when it is on the line end. An empty line keeps it.
fn normal_offset(text: &str, offset: usize) -> usize {
    let start = line_start(text, offset);
    if offset < line_end(text, offset) || offset == start {
        return offset;
    }
    previous_boundary(text, offset).max(start)
}

/// The offset after the char at `offset`, at most the line end.
fn append_offset(text: &str, offset: usize) -> usize {
    next_boundary(text, offset).min(line_end(text, offset))
}

fn left(text: &str, offset: usize, times: usize) -> usize {
    let start = line_start(text, offset);
    let mut at = offset;
    for _ in 0..times {
        if at <= start {
            break;
        }
        at = previous_boundary(text, at);
    }
    at
}

fn right(text: &str, offset: usize, times: usize) -> usize {
    let end = line_end(text, offset);
    let mut at = offset;
    for _ in 0..times {
        let next = next_boundary(text, at);
        if next >= end {
            break;
        }
        at = next;
    }
    at
}

/// The start of the line `times` lines below the one with `offset`, at most the last line.
fn lines_down(text: &str, offset: usize, times: usize) -> usize {
    let mut start = line_start(text, offset);
    for _ in 0..times {
        let end = line_end(text, start);
        if end >= text.len() {
            break;
        }
        start = end + 1;
    }
    start
}

/// The start of the line `times` lines above the one with `offset`, at least the first line.
fn lines_up(text: &str, offset: usize, times: usize) -> usize {
    let mut start = line_start(text, offset);
    for _ in 0..times {
        if start == 0 {
            break;
        }
        start = line_start(text, start - 1);
    }
    start
}

/// The start of line `number` (1 is the first line), at most the last line.
fn line_number(text: &str, number: usize) -> usize {
    lines_down(text, 0, number.saturating_sub(1))
}

/// The first char of the line at `start` that is not a space or a tab.
fn first_non_blank(text: &str, start: usize) -> usize {
    let line = text.get(start..line_end(text, start)).unwrap_or_default();
    let blanks = line.len() - line.trim_start_matches([' ', '\t']).len();
    normal_offset(text, start + blanks)
}

/// How many graphemes come before `offset` on its line.
fn column_of(text: &str, offset: usize) -> usize {
    text.get(line_start(text, offset)..offset)
        .map_or(0, |before| before.graphemes(true).count())
}

/// The grapheme at `column` of the line at `start`, at most its last char.
fn at_column(text: &str, start: usize, column: usize) -> usize {
    let end = line_end(text, start);
    let mut at = start;
    for _ in 0..column {
        let next = next_boundary(text, at);
        if next >= end {
            break;
        }
        at = next;
    }
    at
}

fn class(text: &str, offset: usize) -> Class {
    match text.get(offset..).and_then(|after| after.chars().next()) {
        None => Class::Blank,
        Some(ch) if ch.is_whitespace() => Class::Blank,
        Some(ch) if ch.is_alphanumeric() || ch == '_' => Class::Word,
        Some(_) => Class::Punctuation,
    }
}

/// `offset` is the start of a line with no chars.
fn is_empty_line(text: &str, offset: usize) -> bool {
    let at_newline = text
        .get(offset..)
        .is_some_and(|after| after.starts_with('\n'));
    at_newline && line_start(text, offset) == offset
}

/// `step` `times` from `offset`, then on the last char of the line when it ends on the
/// line end. A step that does not move ends the repeat.
fn repeat(text: &str, offset: usize, times: usize, step: fn(&str, usize) -> usize) -> usize {
    let mut at = offset;
    for _ in 0..times {
        let next = step(text, at);
        if next == at {
            break;
        }
        at = next;
    }
    normal_offset(text, at)
}

/// `w`: the start of the next word or run of punctuation, or of the next empty line.
fn word_start_after(text: &str, offset: usize) -> usize {
    let start = class(text, offset);
    let mut at = offset;
    while start != Class::Blank && at < text.len() && class(text, at) == start {
        at = next_boundary(text, at);
    }
    while at < text.len() && class(text, at) == Class::Blank {
        if at != offset && is_empty_line(text, at) {
            break;
        }
        at = next_boundary(text, at);
    }
    at
}

/// `e`: the last char of the word or run of punctuation that ends after `offset`.
fn word_end_after(text: &str, offset: usize) -> usize {
    let mut at = next_boundary(text, offset);
    while at < text.len() && class(text, at) == Class::Blank {
        at = next_boundary(text, at);
    }
    let run = class(text, at);
    loop {
        let next = next_boundary(text, at);
        if next >= text.len() || class(text, next) != run {
            return at;
        }
        at = next;
    }
}

/// `b`: the start of the word or run of punctuation before `offset`, or of an empty line.
fn word_start_before(text: &str, offset: usize) -> usize {
    let mut at = previous_boundary(text, offset);
    while at > 0 && class(text, at) == Class::Blank && !is_empty_line(text, at) {
        at = previous_boundary(text, at);
    }
    let run = class(text, at);
    if run == Class::Blank {
        return at;
    }
    while at > 0 {
        let before = previous_boundary(text, at);
        if class(text, before) != run {
            break;
        }
        at = before;
    }
    at
}
