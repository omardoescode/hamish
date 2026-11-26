//! Normal mode of Vim: counts, motions, operators with their motion or text object, and the
//! commands that start Insert or Visual mode.

use super::motion::{
    append_offset, at_column, column_of, first_non_blank, forward, left, line_end, line_number,
    lines_down, lines_up, motion_span, normal_offset, repeat, right, word_end_after,
    word_start_after, word_start_before,
};
use super::operator::{Operator, Span};
use super::text_object::text_object;
use super::{
    Prefix, Put, Step, Vim, VimAction, VimCommand, VimKey, VimMode, VimText, char_of, select,
};
use crate::caret::line_start;
use crate::{Caret, CaretEdit, EditKind};

impl Vim {
    pub(super) fn normal_key(
        &mut self,
        key: VimKey,
        text: &VimText<'_>,
        offset: usize,
    ) -> VimCommand {
        self.typed.push(key);
        let step = self.normal_step(key, text, offset);
        self.finish(step)
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
        let operator = self.operator.is_some();
        if let Some(prefix) = Prefix::of(key).filter(|prefix| prefix.in_normal(operator)) {
            self.prefix = Some(prefix);
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
        prefix: Prefix,
        key: VimKey,
        text: &VimText<'_>,
        offset: usize,
    ) -> Step {
        if let Some(step) = self.hamish_prefix(prefix, key, text, offset) {
            return step;
        }
        let operator = self.operator.map(|(operator, _)| operator);
        let count = self.take_count();
        let span = if prefix == Prefix::G {
            gg_target(key, count, text.text).map(|target| Span::Lines(offset, target))
        } else {
            char_of(key).and_then(|object| {
                text_object(
                    text,
                    offset,
                    prefix == Prefix::Around,
                    object,
                    count.unwrap_or(1),
                )
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
            ':' => Step::Done(VimCommand::Run(VimAction::OpenCommandLine)),
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
    pub(super) fn motion(
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

    /// The offset in the line at `start` at the column `j` and `k` keep.
    fn vertical(&mut self, text: &str, offset: usize, start: usize) -> usize {
        let column = *self.column.get_or_insert_with(|| column_of(text, offset));
        at_column(text, start, column)
    }

    pub(super) fn insert_at(&mut self, offset: usize) -> VimCommand {
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

/// The line of `gg`: line `count`, else the first. `None` when `key` is not `g`.
pub(super) fn gg_target(key: VimKey, count: Option<usize>, text: &str) -> Option<usize> {
    (key == VimKey::Char('g')).then(|| first_non_blank(text, line_number(text, count.unwrap_or(1))))
}
