//! Vim mode: the Normal, Insert and Visual modes of the Editor as a key state machine over
//! plain text. Raw keys go in; commands for the caret, the text and the register come out
//! (ADR 0008).

mod hamish;
mod keys;
mod motion;
mod panel;
mod put;
mod text_object;

use std::ops::Range;

use crate::caret::{line_start, next_boundary, previous_boundary};
use crate::{Caret, CaretEdit, Cycle, Document, EditKind, Motion, TrimProposal};
pub use hamish::VimAction;
use hamish::WINDOW_PREFIX;
pub use keys::VIM_KEYS;
use motion::{
    at_column, column_of, first_non_blank, left, line_number, lines_down, lines_up, motion_span,
    repeat, right, word_end_after, word_start_after, word_start_before,
};
pub use panel::{PanelCommand, PanelKeys, VimPanel};
pub use put::Put;
use text_object::text_object;
use unicode_segmentation::UnicodeSegmentation;

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
    /// The first key of a two-key motion or text object: `g`, `i` or `a`.
    prefix: Option<char>,
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
}

/// What an operator does with its span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Delete,
    Change,
    Yank,
    Ghost,
    Revive,
    Stash,
}

impl Operator {
    fn of(key: char) -> Option<Self> {
        match key {
            'd' => Some(Self::Delete),
            'c' => Some(Self::Change),
            'y' => Some(Self::Yank),
            _ => None,
        }
    }

    /// The operator of `g` and `key`.
    fn after_g(key: char) -> Option<Self> {
        match key {
            'h' => Some(Self::Ghost),
            'H' => Some(Self::Revive),
            's' => Some(Self::Stash),
            _ => None,
        }
    }

    /// The key that, typed again, makes the operator act on whole lines: `dd`, `ghgh`.
    const fn key(self) -> char {
        match self {
            Self::Delete => 'd',
            Self::Change => 'c',
            Self::Yank => 'y',
            Self::Ghost => 'h',
            Self::Revive => 'H',
            Self::Stash => 's',
        }
    }

    const fn after_g_only(self) -> bool {
        matches!(self, Self::Ghost | Self::Revive | Self::Stash)
    }
}

/// The text an operator acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Span {
    /// The chars of a byte range.
    Chars(Range<usize>),
    /// Whole lines: from the line that holds the first offset to the line that holds the
    /// second, in either order.
    Lines(usize, usize),
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

/// A change that `.` repeats: its count, its keys after the count, the text typed
/// in the Insert mode it started, and the size of its Visual selection.
#[derive(Debug, Clone, Default)]
struct LastChange {
    count: Option<usize>,
    keys: Vec<VimKey>,
    inserted: String,
    /// `None` for a change made in Normal mode.
    visual: Option<VisualSize>,
}

/// How much text a Visual mode change acted on, so that `.` acts on as much from the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisualSize {
    /// Visual mode in one line: the head is this many graphemes after the anchor.
    Chars(usize),
    /// Visual mode over more lines: the head is `lines` lines down, at `column`.
    Lines { lines: usize, column: usize },
    /// Visual line mode: the head is this many lines down.
    WholeLines(usize),
}

impl VisualSize {
    /// The size of the selection from `anchor` to `head` in `mode`.
    fn of(mode: VimMode, text: &str, anchor: usize, head: usize) -> Self {
        let (low, high) = (anchor.min(head), anchor.max(head));
        let between = text.get(low..high).unwrap_or_default();
        let lines = between.matches('\n').count();
        if mode == VimMode::VisualLine {
            return Self::WholeLines(lines);
        }
        if lines == 0 {
            return Self::Chars(between.graphemes(true).count());
        }
        Self::Lines {
            lines,
            column: column_of(text, high),
        }
    }

    /// The mode and the head char of a selection of this size from `anchor`.
    fn head(self, text: &str, anchor: usize) -> (VimMode, usize) {
        match self {
            Self::Chars(after) => (VimMode::Visual, right(text, anchor, after)),
            Self::Lines { lines, column } => (
                VimMode::Visual,
                at_column(text, lines_down(text, anchor, lines), column),
            ),
            Self::WholeLines(lines) => (VimMode::VisualLine, lines_down(text, anchor, lines)),
        }
    }
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
        match key {
            VimKey::Escape => {
                self.end_insert(text, offset);
                VimCommand::Select(Caret::at(step_back(text, offset)))
            }
            VimKey::Char('k')
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
            VimKey::Char('j') => {
                self.after_j = Some(offset + 'j'.len_utf8());
                VimCommand::PassOn
            }
            VimKey::Char(_)
            | VimKey::Ctrl(_)
            | VimKey::Enter
            | VimKey::Backspace
            | VimKey::Other => VimCommand::PassOn,
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

    fn normal_key(&mut self, key: VimKey, text: &VimText<'_>, offset: usize) -> VimCommand {
        self.typed.push(key);
        let step = self.normal_step(key, text, offset);
        self.finish(step)
    }

    fn visual_key(&mut self, key: VimKey, text: &VimText<'_>, head: usize) -> VimCommand {
        let mode = self.mode;
        let size = VisualSize::of(mode, text.text, self.anchor, head);
        self.typed.push(key);
        let step = self.visual_step(key, text, head);
        let change = matches!(step, Step::Change(_));
        if !matches!(self.mode, VimMode::Visual | VimMode::VisualLine) {
            self.last_visual = Some((mode, self.anchor, self.head));
        }
        let command = self.finish(step);
        if let Some(last) = self.last.as_mut().filter(|_| change && !self.replaying) {
            last.visual = Some(size);
        }
        command
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

    fn record(&mut self, typed: Vec<VimKey>, command: &VimCommand) {
        let mut count: Option<usize> = None;
        let mut keys = typed.into_iter().peekable();
        while let Some(digit) = keys
            .peek()
            .and_then(|key| count_digit(*key, count.is_some()))
        {
            count = Some(count.unwrap_or(0).saturating_mul(10).saturating_add(digit));
            keys.next();
        }
        self.last = Some(LastChange {
            count,
            keys: keys.collect(),
            ..LastChange::default()
        });
        self.insert_start = match command {
            _ if self.mode != VimMode::Insert => None,
            VimCommand::Edit(edit) | VimCommand::Cut { edit, .. } => Some(edit.caret.offset()),
            VimCommand::Select(caret) => Some(caret.offset()),
            VimCommand::PassOn
            | VimCommand::Nothing
            | VimCommand::Yank { .. }
            | VimCommand::Put(_)
            | VimCommand::Undo(_)
            | VimCommand::Redo(_)
            | VimCommand::Ghost(_)
            | VimCommand::Revive(_)
            | VimCommand::Stash(_)
            | VimCommand::CycleSlot { .. }
            | VimCommand::Run(_) => None,
        };
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

    fn normal_step(&mut self, key: VimKey, text: &VimText<'_>, offset: usize) -> Step {
        if let Some(prefix) = self.prefix.take() {
            return self.after_prefix(prefix, key, text, offset);
        }
        if self.take_digit(key) {
            return Step::Pending;
        }
        if key == VimKey::Escape {
            return Step::Done(self.escape(text));
        }
        if self.operator.is_none() && key == VimKey::Ctrl('w') {
            self.prefix = Some(WINDOW_PREFIX);
            return Step::Pending;
        }
        let object = self.operator.is_some() && matches!(key, VimKey::Char('i' | 'a'));
        let hamish = self.operator.is_none() && matches!(key, VimKey::Char(' ' | ']' | '['));
        if object || hamish || key == VimKey::Char('g') {
            self.prefix = char_of(key);
            return Step::Pending;
        }
        match self.operator {
            Some((operator, _)) => self.operator_key(operator, key, text.text, offset),
            None => self.command(key, text, offset),
        }
    }

    /// The motion or the doubled operator key after an operator.
    fn operator_key(&mut self, operator: Operator, key: VimKey, text: &str, offset: usize) -> Step {
        let count = self.take_count();
        if !operator.after_g_only() && key == VimKey::Char(operator.key()) {
            let last = lines_down(text, offset, count.unwrap_or(1) - 1).max(offset);
            return self.operate(operator, Span::Lines(offset, last), text);
        }
        match motion_span(operator, key, count, text, offset) {
            Some(span) => self.operate(operator, span, text),
            None => Step::Done(VimCommand::Nothing),
        }
    }

    /// The key after `g` (a motion), or after `i` or `a` (a text object).
    fn after_prefix(
        &mut self,
        prefix: char,
        key: VimKey,
        text: &VimText<'_>,
        offset: usize,
    ) -> Step {
        if let Some(step) = self.hamish_prefix(prefix, key, text, offset) {
            return step;
        }
        let operator = self.operator.map(|(operator, _)| operator);
        let count = self.take_count();
        let span = if prefix == 'g' {
            gg_target(key, count, text.text).map(|target| Span::Lines(offset, target))
        } else {
            char_of(key).and_then(|object| {
                text_object(text, offset, prefix == 'a', object, count.unwrap_or(1))
            })
        };
        match (operator, span) {
            (Some(operator), Some(span)) => self.operate(operator, span, text.text),
            (None, Some(Span::Lines(_, target))) => {
                self.column = None;
                Step::Done(select(target))
            }
            (None | Some(_), None) | (None, Some(Span::Chars(_))) => {
                Step::Done(VimCommand::Nothing)
            }
        }
    }

    fn command(&mut self, key: VimKey, text: &VimText<'_>, offset: usize) -> Step {
        let count = self.take_count();
        if let Some(step) = self.trim_walk(key, count, text, offset) {
            return step;
        }
        if let Some(target) = self.motion(key, count, text.text, offset) {
            return Step::Done(select(target));
        }
        match key {
            VimKey::Char(letter) => self.letter(letter, count, text, offset),
            VimKey::Ctrl('r') => Step::Done(VimCommand::Redo(count.unwrap_or(1))),
            VimKey::Ctrl(_)
            | VimKey::Escape
            | VimKey::Enter
            | VimKey::Backspace
            | VimKey::Other => Step::Done(VimCommand::Nothing),
        }
    }

    /// The Normal mode command of the char `letter` that is not a motion.
    fn letter(
        &mut self,
        letter: char,
        count: Option<usize>,
        text: &VimText<'_>,
        offset: usize,
    ) -> Step {
        let times = count.unwrap_or(1);
        let plain = text.text;
        let to_line_end =
            || Span::Chars(offset..line_end(plain, lines_down(plain, offset, times - 1)));
        match letter {
            'd' | 'c' | 'y' => {
                self.operator = Operator::of(letter).map(|operator| (operator, count));
                Step::Pending
            }
            'x' => self.operate(
                Operator::Delete,
                Span::Chars(offset..forward(plain, offset, times)),
                plain,
            ),
            'D' => self.operate(Operator::Delete, to_line_end(), plain),
            'C' => self.operate(Operator::Change, to_line_end(), plain),
            'p' => Step::Change(VimCommand::Put(Put::after(offset, times))),
            'P' => Step::Change(VimCommand::Put(Put::before(offset, times))),
            'u' => Step::Done(VimCommand::Undo(times)),
            '.' => self.repeat_change(count, text, offset),
            'v' => self.start_visual(VimMode::Visual, plain, offset),
            'V' => self.start_visual(VimMode::VisualLine, plain, offset),
            'i' => Step::Change(self.insert_at(offset)),
            'a' => Step::Change(self.insert_at(append_offset(plain, offset))),
            'o' => Step::Change(self.open_line(plain, line_end(plain, offset), false)),
            'O' => Step::Change(self.open_line(plain, line_start(plain, offset), true)),
            _ => Step::Done(VimCommand::Nothing),
        }
    }

    /// The caret target of a motion key; `None` when `key` is no motion.
    fn motion(
        &mut self,
        key: VimKey,
        count: Option<usize>,
        text: &str,
        offset: usize,
    ) -> Option<usize> {
        let times = count.unwrap_or(1);
        if !matches!(key, VimKey::Char('j' | 'k')) {
            self.column = None;
        }
        let target = match key {
            VimKey::Char('h') | VimKey::Backspace => left(text, offset, times),
            VimKey::Char('l') => right(text, offset, times),
            VimKey::Char('j') => self.vertical(text, offset, lines_down(text, offset, times)),
            VimKey::Char('k') => self.vertical(text, offset, lines_up(text, offset, times)),
            VimKey::Enter => first_non_blank(text, lines_down(text, offset, times)),
            VimKey::Char('w') => repeat(text, offset, times, word_start_after),
            VimKey::Char('b') => repeat(text, offset, times, word_start_before),
            VimKey::Char('e') => repeat(text, offset, times, word_end_after),
            VimKey::Char('0') => line_start(text, offset),
            VimKey::Char('$') => {
                self.column = Some(usize::MAX);
                normal_offset(text, line_end(text, lines_down(text, offset, times - 1)))
            }
            VimKey::Char('G') => {
                let line =
                    count.map_or_else(|| line_start(text, text.len()), |n| line_number(text, n));
                first_non_blank(text, line)
            }
            VimKey::Char(_) | VimKey::Ctrl(_) | VimKey::Escape | VimKey::Other => return None,
        };
        Some(target)
    }

    /// Runs `operator` over `span` of `text`. The edit is one Undo step; a change
    /// goes on in Insert mode in the same Undo step.
    fn operate(&mut self, operator: Operator, span: Span, text: &str) -> Step {
        match span {
            Span::Chars(range) => self.operate_chars(operator, range, text),
            Span::Lines(one, other) => {
                self.operate_lines(operator, one.min(other), one.max(other), text)
            }
        }
    }

    fn operate_chars(&mut self, operator: Operator, range: Range<usize>, text: &str) -> Step {
        let Some(removed) = text.get(range.clone()).map(str::to_owned) else {
            return Step::Done(VimCommand::Nothing);
        };
        match operator {
            Operator::Delete
            | Operator::Yank
            | Operator::Ghost
            | Operator::Revive
            | Operator::Stash
                if removed.is_empty() =>
            {
                Step::Done(VimCommand::Nothing)
            }
            Operator::Ghost => Step::Change(VimCommand::Ghost(range)),
            Operator::Revive => Step::Change(VimCommand::Revive(range)),
            Operator::Stash => Step::Change(VimCommand::Stash(range)),
            Operator::Delete => {
                let mut edit = CaretEdit::new(range.clone(), "", EditKind::Other);
                edit.caret = Caret::at(normal_offset(&replaced(text, &range, ""), range.start));
                Step::Change(VimCommand::Cut { edit, removed })
            }
            Operator::Change => {
                self.mode = VimMode::Insert;
                let edit = CaretEdit::new(range, "", EditKind::Insert);
                Step::Change(VimCommand::Cut { edit, removed })
            }
            Operator::Yank => Step::Done(VimCommand::Yank {
                text: removed,
                caret: Caret::at(range.start),
            }),
        }
    }

    /// `operator` over the lines from the one with `first` to the one with `last`. The
    /// register gets the lines with a newline after each.
    fn operate_lines(&mut self, operator: Operator, first: usize, last: usize, text: &str) -> Step {
        let start = line_start(text, first);
        let end = line_end(text, last);
        let removed = format!("{}\n", text.get(start..end).unwrap_or_default());
        match operator {
            Operator::Delete => {
                let range = with_newline(text, start..end);
                let after = replaced(text, &range, "");
                let line = line_start(&after, range.start.min(after.len()));
                let mut edit = CaretEdit::new(range, "", EditKind::Other);
                edit.caret = Caret::at(first_non_blank(&after, line));
                Step::Change(VimCommand::Cut { edit, removed })
            }
            Operator::Change => {
                self.mode = VimMode::Insert;
                let edit = CaretEdit::new(start..end, "", EditKind::Insert);
                Step::Change(VimCommand::Cut { edit, removed })
            }
            Operator::Yank => Step::Done(VimCommand::Yank {
                text: removed,
                caret: Caret::at(normal_offset(text, first)),
            }),
            Operator::Ghost => Step::Change(VimCommand::Ghost(start..end)),
            Operator::Revive => Step::Change(VimCommand::Revive(start..end)),
            Operator::Stash => Step::Change(VimCommand::Stash(with_newline(text, start..end))),
        }
    }

    /// `.`: runs the keys of the last change again, with `count` in place of its count,
    /// and puts in the text typed after it as one edit. A Visual mode change acts on a
    /// selection of the same size from `offset`, with no count.
    fn repeat_change(&mut self, count: Option<usize>, text: &VimText<'_>, offset: usize) -> Step {
        let Some(last) = self.last.clone() else {
            return Step::Done(VimCommand::Nothing);
        };
        self.typed.clear();
        self.replaying = true;
        let command = match last.visual {
            Some(size) => self.replay_visual(size, &last.keys, text, offset),
            None => self.replay_normal(count.or(last.count), &last.keys, text, offset),
        };
        self.replaying = false;
        if self.mode != VimMode::Insert {
            return Step::Done(command);
        }
        self.mode = VimMode::Normal;
        Step::Done(with_inserted(command, &last.inserted, text.text))
    }

    fn replay_normal(
        &mut self,
        count: Option<usize>,
        keys: &[VimKey],
        text: &VimText<'_>,
        offset: usize,
    ) -> VimCommand {
        let digits = count.map_or_else(String::new, |count| count.to_string());
        let mut command = VimCommand::Nothing;
        for key in digits.chars().map(VimKey::Char).chain(keys.iter().copied()) {
            command = self.normal_key(key, text, offset);
        }
        command
    }

    /// Runs `keys` in Visual mode over a selection of `size` from `offset`. The selection
    /// of `gv` stays.
    fn replay_visual(
        &mut self,
        size: VisualSize,
        keys: &[VimKey],
        text: &VimText<'_>,
        offset: usize,
    ) -> VimCommand {
        let last_visual = self.last_visual;
        let (mode, head) = size.head(text.text, offset);
        self.mode = mode;
        self.anchor = offset;
        self.head = head;
        let mut command = VimCommand::Nothing;
        for &key in keys {
            command = self.visual_key(key, text, self.head);
        }
        self.last_visual = last_visual;
        command
    }

    fn start_visual(&mut self, mode: VimMode, text: &str, offset: usize) -> Step {
        self.mode = mode;
        self.anchor = offset;
        self.head = offset;
        Step::Done(VimCommand::Select(self.visual_caret(text)))
    }

    /// `v` in Visual mode and `V` in Visual line mode go back to Normal mode; the other
    /// key switches the mode and keeps the selection.
    fn switch_visual(&mut self, mode: VimMode, text: &str, head: usize) -> Step {
        if self.mode == mode {
            self.mode = VimMode::Normal;
            return Step::Done(select(head));
        }
        self.mode = mode;
        Step::Done(VimCommand::Select(self.visual_caret(text)))
    }

    /// The selection of Visual mode: from the anchor char to the head char, both in. In
    /// Visual line mode, the whole lines with no newline at the end.
    fn visual_range(&self, text: &str) -> Range<usize> {
        let (low, high) = (self.anchor.min(self.head), self.anchor.max(self.head));
        if self.mode == VimMode::VisualLine {
            return line_start(text, low)..line_end(text, high);
        }
        low..next_boundary(text, high).min(text.len())
    }

    fn visual_caret(&self, text: &str) -> Caret {
        let range = self.visual_range(text);
        if self.head < self.anchor {
            return Caret::at(range.end).extended(text, Motion::To(range.start));
        }
        Caret::over(range)
    }

    fn visual_step(&mut self, key: VimKey, text: &VimText<'_>, head: usize) -> Step {
        if let Some(prefix) = self.prefix.take() {
            return self.visual_prefix(prefix, key, text, head);
        }
        if self.take_digit(key) {
            return Step::Pending;
        }
        if matches!(key, VimKey::Char('g' | 'i' | 'a' | ']' | '[')) {
            self.prefix = char_of(key);
            return Step::Pending;
        }
        let plain = text.text;
        let count = self.take_count();
        if let Some(target) = self.motion(key, count, plain, head) {
            self.head = target;
            return Step::Done(VimCommand::Select(self.visual_caret(plain)));
        }
        let lines = self.mode == VimMode::VisualLine;
        let operator = match key {
            VimKey::Char('v') => return self.switch_visual(VimMode::Visual, plain, head),
            VimKey::Char('V') => return self.switch_visual(VimMode::VisualLine, plain, head),
            VimKey::Char('p') => {
                let put = Put::over(self.visual_range(plain), lines, count.unwrap_or(1));
                self.mode = VimMode::Normal;
                return Step::Done(VimCommand::Put(put));
            }
            VimKey::Escape => {
                self.mode = VimMode::Normal;
                return Step::Done(select(head));
            }
            VimKey::Char('d' | 'x') => Operator::Delete,
            VimKey::Char('c') => Operator::Change,
            VimKey::Char('y') => Operator::Yank,
            VimKey::Char(_)
            | VimKey::Ctrl(_)
            | VimKey::Enter
            | VimKey::Backspace
            | VimKey::Other => {
                return Step::Done(VimCommand::Nothing);
            }
        };
        self.visual_operate(operator, plain, head)
    }

    /// Runs `operator` over the Visual selection, back in Normal mode.
    fn visual_operate(&mut self, operator: Operator, text: &str, head: usize) -> Step {
        let span = if self.mode == VimMode::VisualLine {
            Span::Lines(self.anchor.min(head), self.anchor.max(head))
        } else {
            Span::Chars(self.visual_range(text))
        };
        self.mode = VimMode::Normal;
        self.operate(operator, span, text)
    }

    /// The key after `g` moves the head; the key after `i` or `a` selects a text object.
    fn visual_prefix(
        &mut self,
        prefix: char,
        key: VimKey,
        text: &VimText<'_>,
        head: usize,
    ) -> Step {
        if let Some(step) = self.visual_hamish_prefix(prefix, key, text, head) {
            return step;
        }
        let count = self.take_count();
        let span = if prefix == 'g' {
            gg_target(key, count, text.text).map(|target| Span::Chars(target..target))
        } else {
            char_of(key).and_then(|object| {
                text_object(text, head, prefix == 'a', object, count.unwrap_or(1))
            })
        };
        match span {
            None => return Step::Done(VimCommand::Nothing),
            Some(Span::Chars(range)) if prefix == 'g' => self.head = range.start,
            Some(Span::Chars(range)) => {
                self.anchor = range.start;
                self.head = previous_boundary(text.text, range.end).max(range.start);
            }
            Some(Span::Lines(first, last)) => {
                self.mode = VimMode::VisualLine;
                self.anchor = first;
                self.head = last;
            }
        }
        Step::Done(VimCommand::Select(self.visual_caret(text.text)))
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
        let mut edit = CaretEdit::new(at..at, "\n", EditKind::Insert);
        if above {
            edit.caret = Caret::at(at);
        }
        VimCommand::Edit(edit)
    }
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

/// The line of `gg`: line `count`, else the first. `None` when `key` is not `g`.
fn gg_target(key: VimKey, count: Option<usize>, text: &str) -> Option<usize> {
    (key == VimKey::Char('g')).then(|| first_non_blank(text, line_number(text, count.unwrap_or(1))))
}

/// `times` graphemes after `offset`, at most the line end.
fn forward(text: &str, offset: usize, times: usize) -> usize {
    let end = line_end(text, offset);
    let mut at = offset;
    for _ in 0..times {
        if at >= end {
            break;
        }
        at = next_boundary(text, at);
    }
    at.min(end)
}

/// The lines in `lines` with the newline after them; the last line takes the newline before it.
fn with_newline(text: &str, lines: Range<usize>) -> Range<usize> {
    if lines.end < text.len() {
        return lines.start..lines.end + 1;
    }
    lines.start.saturating_sub(1)..lines.end
}

/// `text` with `range` replaced by `new`.
fn replaced(text: &str, range: &Range<usize>, new: &str) -> String {
    let before = text.get(..range.start).unwrap_or_default();
    let after = text.get(range.end..).unwrap_or_default();
    format!("{before}{new}{after}")
}

/// The edit of a change that started Insert mode, with `inserted` typed at its caret, back
/// in Normal mode.
fn with_inserted(command: VimCommand, inserted: &str, text: &str) -> VimCommand {
    let (edit, removed) = match command {
        VimCommand::Edit(edit) => (edit, None),
        VimCommand::Cut { edit, removed } => (edit, Some(removed)),
        VimCommand::Select(caret) => {
            let at = caret.offset();
            (CaretEdit::new(at..at, "", EditKind::Other), None)
        }
        other @ (VimCommand::PassOn
        | VimCommand::Nothing
        | VimCommand::Yank { .. }
        | VimCommand::Put(_)
        | VimCommand::Undo(_)
        | VimCommand::Redo(_)
        | VimCommand::Ghost(_)
        | VimCommand::Revive(_)
        | VimCommand::Stash(_)
        | VimCommand::CycleSlot { .. }
        | VimCommand::Run(_)) => return other,
    };
    let split = edit.caret.offset().saturating_sub(edit.range.start);
    let before = edit.text.get(..split).unwrap_or(&edit.text);
    let after = edit.text.get(split..).unwrap_or_default();
    let new = format!("{before}{inserted}{after}");
    let end = edit.range.start + before.len() + inserted.len();
    let mut done = CaretEdit::new(edit.range.clone(), &new, EditKind::Other);
    done.caret = Caret::at(step_back(&replaced(text, &edit.range, &new), end));
    match removed {
        Some(removed) => VimCommand::Cut {
            edit: done,
            removed,
        },
        None => VimCommand::Edit(done),
    }
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
