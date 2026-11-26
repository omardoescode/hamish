//! The hamishrc file: a Vimscript subset with the key maps of Vim mode and the leader. Text
//! goes in; the maps and the line-numbered errors come out.

use std::fmt;

use super::{VimAction, VimKey};

/// The text of a new hamishrc: each line is a comment, so it changes no key.
pub const HAMISHRC_EXAMPLE: &str = r#"" hamishrc: the Vim mode keys of Hamish. It applies only with Vim mode on.
" A line that starts with a double quote is a comment.
" Save, then go back to Hamish, or type :source, to read it again.

" The leader is Space. A comma as the leader:
" let mapleader = ","

" Normal mode: Y yanks to the line end.
" nnoremap Y y$

" Insert mode: jj goes back to Normal mode.
" inoremap jj <Esc>

" A Hamish action by name:
" nnoremap <leader>s :Stash

" A Trim level by name: :RunTrim1 (Slight trim), :RunTrim2 (Tighten more),
" :RunTrim3 (Even sharper), :RunTrim4 (Cut in half).
" nnoremap <leader>2 :RunTrim2

" :Quit closes Hamish as :q does; :ForceQuit drops unsaved changes, as :q!.
" nnoremap <leader>q :Quit
"#;

/// The Vim mode states that have maps of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapMode {
    Normal,
    Insert,
    /// Visual mode and Visual line mode.
    Visual,
}

/// What a map gives in place of its keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapTarget {
    Keys(Vec<VimKey>),
    /// The Hamish action that `:Name` names.
    Action(VimAction),
}

/// One map of a hamishrc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyMap {
    pub(crate) mode: MapMode,
    pub(crate) keys: Vec<VimKey>,
    pub(crate) target: MapTarget,
    /// `nmap`: the maps apply again to the target keys. `nnoremap`: they do not.
    pub(crate) recursive: bool,
}

/// Why a hamishrc line does not load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HamishrcProblem {
    /// The first word is no hamishrc command.
    UnknownCommand(String),
    /// A map or an unmap with no keys.
    NoKeys,
    /// A map with keys and no target.
    NoTarget,
    /// A `<...>` name that is no key.
    UnknownKey(String),
    /// `:Name` names no Hamish action.
    UnknownAction(String),
    /// An unmap of keys that have no map in its mode.
    NoMap(String),
    /// Text after the keys of an unmap.
    ExtraText(String),
    /// `let` of a variable other than `mapleader`, or with no `=`.
    UnknownVariable(String),
    /// A `mapleader` value that is not one quoted key that types a char.
    BadLeader(String),
}

impl fmt::Display for HamishrcProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(word) => write!(
                f,
                "Not a hamishrc command: {word} (use nmap, nnoremap, imap, inoremap, vmap, vnoremap, nunmap, iunmap, vunmap or let mapleader)"
            ),
            Self::NoKeys => write!(f, "No keys after the command"),
            Self::NoTarget => write!(f, "No keys or :Action after the keys"),
            Self::UnknownKey(name) => write!(f, "Not a key: {name}"),
            Self::UnknownAction(name) => write!(f, "Not a Hamish action: {name}"),
            Self::NoMap(keys) => write!(f, "No map to remove: {keys}"),
            Self::ExtraText(text) => write!(f, "Text after the keys: {text}"),
            Self::UnknownVariable(name) => {
                write!(f, "Not a hamishrc variable: {name} (use mapleader)")
            }
            Self::BadLeader(value) => write!(f, "The leader must be one quoted key: {value}"),
        }
    }
}

/// A hamishrc line that does not load, with its line number (from 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HamishrcError {
    pub line: usize,
    pub problem: HamishrcProblem,
}

impl fmt::Display for HamishrcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "hamishrc line {}: {}", self.line, self.problem)
    }
}

impl std::error::Error for HamishrcError {}

/// The maps and the leader of a hamishrc, and the errors of the lines that do not load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hamishrc {
    leader: char,
    maps: Vec<KeyMap>,
    errors: Vec<HamishrcError>,
}

impl Default for Hamishrc {
    /// No maps; Space is the leader.
    fn default() -> Self {
        Self {
            leader: ' ',
            maps: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl Hamishrc {
    /// Reads the hamishrc `text`, line by line. A line with an error does not load; the
    /// other lines do. `<leader>` is the leader at its line.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut rc = Self::default();
        for (index, line) in text.lines().enumerate() {
            if let Err(problem) = rc.read_line(line) {
                rc.errors.push(HamishrcError {
                    line: index + 1,
                    problem,
                });
            }
        }
        rc
    }

    /// The leader: the key that starts the Hamish keys of Normal mode and that `<leader>`
    /// names.
    #[must_use]
    pub fn leader(&self) -> char {
        self.leader
    }

    /// The lines that did not load, in order.
    #[must_use]
    pub fn errors(&self) -> &[HamishrcError] {
        &self.errors
    }

    pub(crate) fn maps(&self, mode: MapMode) -> impl Iterator<Item = &KeyMap> {
        self.maps.iter().filter(move |map| map.mode == mode)
    }

    fn read_line(&mut self, line: &str) -> Result<(), HamishrcProblem> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('"') {
            return Ok(());
        }
        let (command, rest) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(command, rest)| (command, rest.trim_start()));
        if command == "let" {
            return self.let_variable(rest);
        }
        let (mode, kind) = map_command(command)
            .ok_or_else(|| HamishrcProblem::UnknownCommand(command.to_owned()))?;
        let (typed, target) = rest
            .split_once(char::is_whitespace)
            .map_or((rest, ""), |(typed, target)| (typed, target.trim()));
        if typed.is_empty() {
            return Err(HamishrcProblem::NoKeys);
        }
        let keys = parse_keys(typed, self.leader)?;
        let Some(recursive) = kind else {
            return self.unmap(mode, &keys, typed, target);
        };
        if target.is_empty() {
            return Err(HamishrcProblem::NoTarget);
        }
        let target = parse_target(target, self.leader)?;
        self.maps
            .retain(|map| !(map.mode == mode && map.keys == keys));
        self.maps.push(KeyMap {
            mode,
            keys,
            target,
            recursive,
        });
        Ok(())
    }

    fn unmap(
        &mut self,
        mode: MapMode,
        keys: &[VimKey],
        typed: &str,
        extra: &str,
    ) -> Result<(), HamishrcProblem> {
        if !extra.is_empty() {
            return Err(HamishrcProblem::ExtraText(extra.to_owned()));
        }
        let before = self.maps.len();
        self.maps
            .retain(|map| !(map.mode == mode && map.keys == keys));
        if self.maps.len() == before {
            return Err(HamishrcProblem::NoMap(typed.to_owned()));
        }
        Ok(())
    }

    /// `let mapleader = ","`: the leader of the lines after it, and of Normal mode.
    fn let_variable(&mut self, rest: &str) -> Result<(), HamishrcProblem> {
        let Some((name, value)) = rest.split_once('=') else {
            return Err(HamishrcProblem::UnknownVariable(rest.to_owned()));
        };
        let name = name.trim();
        if !matches!(name, "mapleader" | "g:mapleader") {
            return Err(HamishrcProblem::UnknownVariable(name.to_owned()));
        }
        let value = value.trim();
        let bad = || HamishrcProblem::BadLeader(value.to_owned());
        let keys = quoted(value).ok_or_else(bad)?;
        match keys.as_slice() {
            [VimKey::Char(leader)] => {
                self.leader = *leader;
                Ok(())
            }
            _ => Err(bad()),
        }
    }
}

/// The mode of a map command, and `Some(recursive)` for a map or `None` for an unmap.
fn map_command(command: &str) -> Option<(MapMode, Option<bool>)> {
    let found = match command {
        "nmap" => (MapMode::Normal, Some(true)),
        "nnoremap" => (MapMode::Normal, Some(false)),
        "nunmap" => (MapMode::Normal, None),
        "imap" => (MapMode::Insert, Some(true)),
        "inoremap" => (MapMode::Insert, Some(false)),
        "iunmap" => (MapMode::Insert, None),
        "vmap" => (MapMode::Visual, Some(true)),
        "vnoremap" => (MapMode::Visual, Some(false)),
        "vunmap" => (MapMode::Visual, None),
        _ => return None,
    };
    Some(found)
}

/// `:Name` (with an optional `<CR>`) is an action; other text is keys.
fn parse_target(target: &str, leader: char) -> Result<MapTarget, HamishrcProblem> {
    let Some(name) = target.strip_prefix(':') else {
        return parse_keys(target, leader).map(MapTarget::Keys);
    };
    let name = strip_suffix_ignore_case(name, "<cr>");
    VimAction::named(name)
        .map(MapTarget::Action)
        .ok_or_else(|| HamishrcProblem::UnknownAction(name.to_owned()))
}

fn strip_suffix_ignore_case<'a>(text: &'a str, suffix: &str) -> &'a str {
    let split = text.len().saturating_sub(suffix.len());
    match text.split_at_checked(split) {
        Some((before, end)) if end.eq_ignore_ascii_case(suffix) => before,
        Some(_) | None => text,
    }
}

/// The key of a `mapleader` value: `","`, `' '`, or `"\<Space>"`. `None` for a value that is
/// not quoted.
fn quoted(value: &str) -> Option<Vec<VimKey>> {
    if let Some(inner) = value
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return Some(inner.chars().map(VimKey::Char).collect());
    }
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    if let Some(name) = inner
        .strip_prefix('\\')
        .filter(|name| name.starts_with('<'))
    {
        return parse_keys(name, ' ').ok();
    }
    Some(
        inner
            .replace("\\\\", "\\")
            .chars()
            .map(VimKey::Char)
            .collect(),
    )
}

/// The keys of `typed` in Vim key notation: `<leader>`, `<C-x>`, `<CR>`, `<Esc>`, `<BS>`,
/// `<Space>`, `<lt>`, `<Bar>`, `<Bslash>` (any case), and plain chars. A `<` that starts
/// no name is a plain `<`.
fn parse_keys(typed: &str, leader: char) -> Result<Vec<VimKey>, HamishrcProblem> {
    let mut keys = Vec::new();
    let mut rest = typed;
    while let Some(first) = rest.chars().next() {
        let named = rest.strip_prefix('<').and_then(|after| {
            let end = after.find('>')?;
            let name = after.get(..end)?;
            let is_name = !name.is_empty()
                && name
                    .chars()
                    .all(|typed| typed.is_ascii_alphanumeric() || typed == '-');
            is_name.then_some((name, after.get(end + 1..).unwrap_or_default()))
        });
        let Some((name, after)) = named else {
            keys.push(VimKey::Char(first));
            rest = rest.get(first.len_utf8()..).unwrap_or_default();
            continue;
        };
        let key = named_key(name, leader)
            .ok_or_else(|| HamishrcProblem::UnknownKey(format!("<{name}>")))?;
        keys.push(key);
        rest = after;
    }
    Ok(keys)
}

fn named_key(name: &str, leader: char) -> Option<VimKey> {
    let lower = name.to_ascii_lowercase();
    if let Some(ctrl) = lower.strip_prefix("c-") {
        return super::one_char(ctrl).map(VimKey::Ctrl);
    }
    let key = match lower.as_str() {
        "leader" => VimKey::Char(leader),
        "cr" | "enter" | "return" => VimKey::Enter,
        "esc" => VimKey::Escape,
        "bs" => VimKey::Backspace,
        "space" => VimKey::Char(' '),
        "lt" => VimKey::Char('<'),
        "bar" => VimKey::Char('|'),
        "bslash" => VimKey::Char('\\'),
        _ => return None,
    };
    Some(key)
}
