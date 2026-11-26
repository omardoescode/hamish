//! The Command line of Vim mode: the line `:` opens, its history, Tab completion, and the
//! grammar of its commands. Keys go in; a command or an error comes out (ADR 0008).

use std::fmt;
use std::path::{Path, PathBuf};

use crate::caret::{next_boundary, previous_boundary};

/// The command names that Tab completes, in order.
const NAMES: [&str; 9] = [
    "chrome", "edit", "quit", "set", "settings", "source", "write", "wq", "x",
];

/// The settings that `:set` changes.
const SETTINGS: [&str; 2] = ["novim", "vim"];

/// One key press on the Command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKey {
    /// A key that types this char.
    Char(char),
    Backspace,
    Tab,
    Up,
    Down,
    Enter,
    Escape,
    /// Move the caret one char back.
    Left,
    /// Move the caret one char on.
    Right,
    /// Move the caret to the line start.
    Home,
    /// Move the caret to the line end.
    End,
    /// Delete the blanks, then the word or the run of other signs, before the caret.
    DeleteWord,
    /// Delete the text before the caret.
    DeleteToStart,
}

impl LineKey {
    /// The key of Ctrl with `key`: `b` Home, `e` End, `w` the word before the caret, `u`
    /// the text before the caret. `None` for another key.
    #[must_use]
    pub fn ctrl(key: &str) -> Option<Self> {
        match key {
            "b" => Some(Self::Home),
            "e" => Some(Self::End),
            "w" => Some(Self::DeleteWord),
            "u" => Some(Self::DeleteToStart),
            _ => None,
        }
    }
}

/// What the Command line does after one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineStep {
    /// The line stays open.
    Open,
    /// The line closes with no command.
    Closed,
    /// The line closes: run the command, or show the error as a Notice.
    Run(Result<LineCommand, LineError>),
    /// Tab on a path: list the entries of this directory, as typed, and give them to
    /// [`CommandLine::complete_paths`]. Empty text is the base directory of
    /// [`resolve_path`]. The line stays open.
    ListDirectory(String),
}

/// A command of the Command line. A path is as the writer typed it: [`resolve_path`] makes
/// it a file path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineCommand {
    /// `:w`: save to the path of the Document, or to this path.
    Write(Option<String>),
    /// `:q`, or `:q!` with `force`: close Hamish.
    Quit { force: bool },
    /// `:wq` and `:x`: save, then close Hamish. With `only_unsaved` (`:x`), save only when
    /// the Document has unsaved changes.
    WriteQuit {
        path: Option<String>,
        only_unsaved: bool,
    },
    /// `:e`: open this `.md` file, or the Document file again. With `force` (`:e!`), drop
    /// unsaved changes.
    Edit { path: Option<String>, force: bool },
    /// `:set vim` and `:set novim`: turn the Vim mode setting on or off.
    SetVimMode(bool),
    /// `:chrome`: turn the Chrome bar off or on.
    ToggleChrome,
    /// `:settings`: open Settings.
    OpenSettings,
    /// `:source`: read the hamishrc again.
    ReadHamishrc,
}

/// Why a command of the Command line does not run. Its text is the Notice text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineError {
    /// No command has this name.
    Unknown(String),
    /// `:set` has no setting of this name.
    UnknownSetting(String),
    /// `:set` with no setting.
    NoSetting,
    /// Text after a command that takes none.
    ExtraText(String),
    /// The command drops unsaved changes, and has no `!`.
    Unsaved,
    /// The command needs the Document path, and the Document has none.
    NoFileName,
}

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "Not a command: {name}"),
            Self::UnknownSetting(setting) => {
                write!(f, "Not a setting: {setting} (use vim or novim)")
            }
            Self::NoSetting => write!(f, "Set what? Use :set vim or :set novim"),
            Self::ExtraText(text) => write!(f, "Text after the command: {text}"),
            Self::Unsaved => write!(f, "Unsaved changes: add ! to drop them"),
            Self::NoFileName => write!(f, "No file name: add a path"),
        }
    }
}

impl std::error::Error for LineError {}

impl LineCommand {
    /// The error that stops this command for a Document with `unsaved` changes and with a
    /// path (`has_path`); `None` when the command can run.
    #[must_use]
    pub fn refusal(&self, unsaved: bool, has_path: bool) -> Option<LineError> {
        match self {
            Self::Quit { force: false } | Self::Edit { force: false, .. } if unsaved => {
                Some(LineError::Unsaved)
            }
            Self::Edit { path: None, .. } if !has_path => Some(LineError::NoFileName),
            Self::WriteQuit {
                path: None,
                only_unsaved,
            } if !has_path && (unsaved || !only_unsaved) => Some(LineError::NoFileName),
            Self::Write(_)
            | Self::Quit { .. }
            | Self::WriteQuit { .. }
            | Self::Edit { .. }
            | Self::SetVimMode(_)
            | Self::ToggleChrome
            | Self::OpenSettings
            | Self::ReadHamishrc => None,
        }
    }
}

/// One entry of a directory, for path completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub name: String,
    pub directory: bool,
}

/// The Command line: its text and caret, the history of this session, and the Tab
/// candidates.
#[derive(Debug, Clone, Default)]
pub struct CommandLine {
    text: String,
    /// The caret, a byte offset of `text` on a char boundary.
    caret: usize,
    /// The lines run in this session, oldest first, with no repeat of the line before.
    history: Vec<String>,
    /// The history entry that Up or Down shows; `None` while the writer types.
    recalled: Option<usize>,
    /// The text typed before the first Up.
    typed: String,
    completion: Option<Completion>,
}

/// The candidates of the last Tab.
#[derive(Debug, Clone)]
struct Completion {
    /// The text before the completed word.
    stem: String,
    candidates: Vec<String>,
    /// The candidate on the line.
    at: usize,
}

impl CommandLine {
    /// The text after the `:`.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The caret: a byte offset of [`Self::text`].
    #[must_use]
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Reads `key`. Enter and Escape close the line and clear it; Backspace on an empty
    /// line closes it. Tab and Up or Down put the caret at the line end.
    pub fn key(&mut self, key: LineKey) -> LineStep {
        if key != LineKey::Tab {
            self.completion = None;
        }
        match key {
            LineKey::Char(typed) => {
                self.recalled = None;
                self.text.insert(self.caret, typed);
                self.caret += typed.len_utf8();
                LineStep::Open
            }
            LineKey::Backspace => self.backspace(),
            LineKey::Left => self.move_caret(previous_boundary(&self.text, self.caret)),
            LineKey::Right => self.move_caret(next_boundary(&self.text, self.caret)),
            LineKey::Home => self.move_caret(0),
            LineKey::End => self.move_caret(self.text.len()),
            LineKey::DeleteWord => self.delete_back_to(self.word_start()),
            LineKey::DeleteToStart => self.delete_back_to(0),
            LineKey::Tab => self.tab(),
            LineKey::Up => {
                self.recall_older();
                LineStep::Open
            }
            LineKey::Down => {
                self.recall_newer();
                LineStep::Open
            }
            LineKey::Enter => self.submit(),
            LineKey::Escape => {
                self.clear();
                LineStep::Closed
            }
        }
    }

    /// Completes the path on the line from `entries`, the entries of the directory
    /// `directory` that [`LineStep::ListDirectory`] asked for. Entries that start with a dot
    /// count only after a typed dot. A line that changed since that Tab stays as it is.
    pub fn complete_paths(&mut self, directory: &str, entries: Vec<DirectoryEntry>) {
        let Some((stem, typed_directory, prefix)) = self.path_parts() else {
            return;
        };
        if typed_directory != directory {
            return;
        }
        let mut candidates: Vec<String> = entries
            .into_iter()
            .filter(|entry| {
                entry.name.starts_with(prefix)
                    && (prefix.starts_with('.') || !entry.name.starts_with('.'))
            })
            .map(|entry| {
                let slash = if entry.directory { "/" } else { "" };
                format!("{typed_directory}{}{slash}", entry.name)
            })
            .collect();
        candidates.sort();
        let stem = stem.to_owned();
        self.offer(stem, candidates);
    }

    fn backspace(&mut self) -> LineStep {
        if self.text.is_empty() {
            self.recalled = None;
            return LineStep::Closed;
        }
        self.delete_back_to(previous_boundary(&self.text, self.caret))
    }

    fn move_caret(&mut self, to: usize) -> LineStep {
        self.caret = to.min(self.text.len());
        LineStep::Open
    }

    /// Deletes the text from `start` to the caret.
    fn delete_back_to(&mut self, start: usize) -> LineStep {
        self.recalled = None;
        let start = start.min(self.caret);
        if self.text.is_char_boundary(start) && self.text.is_char_boundary(self.caret) {
            self.text.replace_range(start..self.caret, "");
            self.caret = start;
        }
        LineStep::Open
    }

    /// The start of the blanks and the word, or the run of other signs, before the caret.
    fn word_start(&self) -> usize {
        let before = self.text.get(..self.caret).unwrap_or_default().trim_end();
        let Some(last) = before.chars().next_back() else {
            return 0;
        };
        let word = is_word(last);
        before
            .char_indices()
            .rev()
            .take_while(|&(_, typed)| !typed.is_whitespace() && is_word(typed) == word)
            .last()
            .map_or(before.len(), |(start, _)| start)
    }

    /// Puts `text` on the line with the caret at its end.
    fn set_text(&mut self, text: String) {
        self.caret = text.len();
        self.text = text;
    }

    fn clear(&mut self) {
        self.caret = 0;
        self.text.clear();
        self.recalled = None;
        self.completion = None;
    }

    fn submit(&mut self) -> LineStep {
        let line = std::mem::take(&mut self.text);
        self.clear();
        let line = line.trim();
        if line.is_empty() {
            return LineStep::Closed;
        }
        if self.history.last().is_none_or(|last| last != line) {
            self.history.push(line.to_owned());
        }
        LineStep::Run(parse(line))
    }

    fn recall_older(&mut self) {
        let index = match self.recalled {
            Some(index) => index.saturating_sub(1),
            None if self.history.is_empty() => return,
            None => {
                self.typed.clone_from(&self.text);
                self.history.len() - 1
            }
        };
        self.show_recalled(index);
    }

    fn recall_newer(&mut self) {
        let Some(index) = self.recalled else {
            return;
        };
        if index + 1 < self.history.len() {
            self.show_recalled(index + 1);
            return;
        }
        self.recalled = None;
        self.set_text(self.typed.clone());
    }

    fn show_recalled(&mut self, index: usize) {
        if let Some(entry) = self.history.get(index).cloned() {
            self.set_text(entry);
            self.recalled = Some(index);
        }
    }

    /// A second Tab cycles the candidates of the first; else Tab finds new ones.
    fn tab(&mut self) -> LineStep {
        if let Some(completion) = self
            .completion
            .as_mut()
            .filter(|done| done.candidates.len() > 1)
        {
            completion.at = (completion.at + 1) % completion.candidates.len();
            let candidate = completion
                .candidates
                .get(completion.at)
                .map_or("", String::as_str);
            let text = format!("{}{candidate}", completion.stem);
            self.set_text(text);
            return LineStep::Open;
        }
        self.completion = None;
        if let Some((_, directory, _)) = self.path_parts() {
            return LineStep::ListDirectory(directory.to_owned());
        }
        let (stem, word, choices): (&str, &str, &[&str]) = match self.text.split_once(' ') {
            None => ("", self.text.as_str(), &NAMES),
            Some(("set" | "se", setting)) => (
                self.text
                    .get(..=self.text.len() - setting.len() - 1)
                    .unwrap_or_default(),
                setting.trim_start(),
                &SETTINGS,
            ),
            Some(_) => return LineStep::Open,
        };
        let stem = stem.to_owned();
        let candidates = choices
            .iter()
            .filter(|choice| choice.starts_with(word))
            .map(|&choice| choice.to_owned())
            .collect();
        self.offer(stem, candidates);
        LineStep::Open
    }

    /// Puts the first of `candidates` after `stem`; with none, the line stays.
    fn offer(&mut self, stem: String, candidates: Vec<String>) {
        let Some(first) = candidates.first() else {
            return;
        };
        self.set_text(format!("{stem}{first}"));
        self.completion = Some(Completion {
            stem,
            candidates,
            at: 0,
        });
    }

    /// For a command that takes a path: the text up to the path, the typed directory (up
    /// to its last `/`), and the typed start of the entry name.
    fn path_parts(&self) -> Option<(&str, &str, &str)> {
        let (name, path) = self.text.split_once(' ')?;
        if !matches!(
            name.trim_end_matches('!'),
            "e" | "edit" | "w" | "write" | "wq" | "x"
        ) {
            return None;
        }
        let path = path.trim_start();
        let stem = self.text.get(..self.text.len() - path.len())?;
        let split = path.rfind('/').map_or(0, |slash| slash + 1);
        Some((stem, path.get(..split)?, path.get(split..)?))
    }
}

/// A char of a word for [`LineKey::DeleteWord`]: a letter, a digit or `_`.
fn is_word(typed: char) -> bool {
    typed.is_alphanumeric() || typed == '_'
}

/// The command of `line`, the text after the `:`.
fn parse(line: &str) -> Result<LineCommand, LineError> {
    let line = line.trim();
    let name_end = line
        .find(|typed: char| !typed.is_ascii_alphabetic())
        .unwrap_or(line.len());
    let (name, rest) = line.split_at(name_end);
    let (force, rest) = rest
        .strip_prefix('!')
        .map_or((false, rest), |after| (true, after));
    let argument = Some(rest.trim()).filter(|argument| !argument.is_empty());
    let path = argument.map(str::to_owned);
    let no_argument = |command| match argument {
        Some(extra) => Err(LineError::ExtraText(extra.to_owned())),
        None => Ok(command),
    };
    match name {
        "w" | "write" => Ok(LineCommand::Write(path)),
        "q" | "quit" => no_argument(LineCommand::Quit { force }),
        "wq" => Ok(LineCommand::WriteQuit {
            path,
            only_unsaved: false,
        }),
        "x" | "xit" => Ok(LineCommand::WriteQuit {
            path,
            only_unsaved: true,
        }),
        "e" | "edit" => Ok(LineCommand::Edit { path, force }),
        "set" | "se" => set_setting(argument),
        "chrome" => no_argument(LineCommand::ToggleChrome),
        "settings" => no_argument(LineCommand::OpenSettings),
        "so" | "source" => no_argument(LineCommand::ReadHamishrc),
        _ => Err(LineError::Unknown(
            line.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned(),
        )),
    }
}

/// `:set` with `setting`.
fn set_setting(setting: Option<&str>) -> Result<LineCommand, LineError> {
    match setting {
        Some("vim") => Ok(LineCommand::SetVimMode(true)),
        Some("novim") => Ok(LineCommand::SetVimMode(false)),
        Some(setting) => Err(LineError::UnknownSetting(setting.to_owned())),
        None => Err(LineError::NoSetting),
    }
}

/// The file path of `typed`: `~/` starts at `home`, and a relative path starts at
/// `document_folder` (the folder of the Document file), else at `home`.
#[must_use]
pub fn resolve_path(typed: &str, home: Option<&Path>, document_folder: Option<&Path>) -> PathBuf {
    if let (Some(after), Some(home)) = (typed.strip_prefix("~/"), home) {
        return home.join(after);
    }
    if let Some(home) = home.filter(|_| typed == "~") {
        return home.to_owned();
    }
    let typed = Path::new(typed);
    if typed.is_absolute() {
        return typed.to_owned();
    }
    document_folder
        .or(home)
        .map_or_else(|| typed.to_owned(), |base| base.join(typed))
}
