//! Visual mode of Vim: the selection from the anchor char to the head char, of chars or of
//! whole lines, and the operators and puts that act on it.

use std::ops::Range;

use super::motion::line_end;
use super::normal::gg_target;
use super::operator::{Operator, Span};
use super::repeat::VisualSize;
use super::text_object::text_object;
use super::{Put, Step, Vim, VimCommand, VimKey, VimMode, VimText, char_of, select};
use crate::caret::{line_start, next_boundary, previous_boundary};
use crate::{Caret, Motion};

impl Vim {
    pub(super) fn visual_key(
        &mut self,
        key: VimKey,
        text: &VimText<'_>,
        head: usize,
    ) -> VimCommand {
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

    pub(super) fn start_visual(&mut self, mode: VimMode, text: &str, offset: usize) -> Step {
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

    pub(super) fn visual_caret(&self, text: &str) -> Caret {
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
    pub(super) fn visual_operate(&mut self, operator: Operator, text: &str, head: usize) -> Step {
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
}
