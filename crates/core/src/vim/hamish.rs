//! The Hamish keys of Vim mode: the leader (Space), the `]` and `[` jumps and Slot cycle,
//! and `g` with Ghost, Revive, Stash and `gv`.

use std::ops::Range;

use super::motion::lines_down;
use super::{Operator, Span, Step, Vim, VimCommand, VimKey, VimText};
use super::{char_of, select};
use crate::{Caret, Cycle, TrimLevel};

/// An action of Hamish that a Vim mode key runs: the Editor runs the same GPUI action as
/// its key binding or menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimAction {
    /// Open the Alternatives panel on the target.
    OpenAlternatives,
    /// Open or close the Lab popover.
    ToggleLab,
    /// Run a Trim at this Trim level.
    RunTrim(TrimLevel),
    /// Put in the fix of the Lab mark at the caret.
    AcceptLabMark,
    /// End the Lab mark at the caret; the text stays.
    DismissLabMark,
    /// Keep the text of the Trim cut at the caret.
    KeepTrimCut,
}

impl Vim {
    /// The command of `key` after the prefix `prefix` in Normal mode, when the prefix is
    /// a Hamish key. `None` leaves the key to the Vim motions and text objects.
    pub(super) fn hamish_prefix(
        &mut self,
        prefix: char,
        key: VimKey,
        text: &VimText<'_>,
        offset: usize,
    ) -> Option<Step> {
        match prefix {
            ' ' => Some(self.leader(key)),
            ']' | '[' => Some(self.bracket(prefix == ']', key, text, offset)),
            'g' => self.after_g(key, text.text, offset),
            _ => None,
        }
    }

    /// As [`Vim::hamish_prefix`], in Visual mode: the jumps move the head, and Ghost,
    /// Revive and Stash act on the selection.
    pub(super) fn visual_hamish_prefix(
        &mut self,
        prefix: char,
        key: VimKey,
        text: &VimText<'_>,
        head: usize,
    ) -> Option<Step> {
        if matches!(prefix, ']' | '[') {
            let times = self.take_count().unwrap_or(1);
            let Some(target) = jump_target(key, text, head, prefix == ']', times) else {
                return Some(Step::Done(VimCommand::Nothing));
            };
            self.head = target;
            return Some(Step::Done(VimCommand::Select(self.visual_caret(text.text))));
        }
        if prefix != 'g' {
            return None;
        }
        if key == VimKey::Char('v') {
            return Some(Step::Done(VimCommand::Nothing));
        }
        let operator = char_of(key).and_then(Operator::after_g)?;
        Some(self.visual_operate(operator, text.text, head))
    }

    /// The key after the leader runs a Hamish action. A count picks the Trim level of
    /// `t`: 1 Slight trim (also with no count) to 4 Cut in half.
    fn leader(&mut self, key: VimKey) -> Step {
        let count = self.take_count();
        let Some(typed) = char_of(key) else {
            return Step::Done(VimCommand::Nothing);
        };
        let action = match typed {
            'a' => VimAction::OpenAlternatives,
            'l' => VimAction::ToggleLab,
            't' => VimAction::RunTrim(trim_level(count)),
            'y' => return Step::Change(VimCommand::Run(VimAction::AcceptLabMark)),
            'x' => return Step::Change(VimCommand::Run(VimAction::DismissLabMark)),
            'k' => return Step::Change(VimCommand::Run(VimAction::KeepTrimCut)),
            _ => return Step::Done(VimCommand::Nothing),
        };
        Step::Done(VimCommand::Run(action))
    }

    /// `]v` and `[v` cycle the Slot at the caret; `]s`, `[s`, `]m` and `[m` jump.
    fn bracket(&mut self, forward: bool, key: VimKey, text: &VimText<'_>, offset: usize) -> Step {
        let times = self.take_count().unwrap_or(1);
        let cycle = if forward {
            Cycle::Next
        } else {
            Cycle::Previous
        };
        if key == VimKey::Char('v') {
            return Step::Done(VimCommand::CycleSlot { cycle, times });
        }
        let Some(target) = jump_target(key, text, offset, forward, times) else {
            return Step::Done(VimCommand::Nothing);
        };
        self.column = None;
        Step::Done(select(target))
    }

    /// The key after `g`: an operator, the doubled key of the operator, or `gv`. `None`
    /// for any other key.
    fn after_g(&mut self, key: VimKey, text: &str, offset: usize) -> Option<Step> {
        let typed = char_of(key)?;
        let Some((operator, _)) = self.operator else {
            return self.g_command(typed, text);
        };
        (operator.after_g_only() && operator.key() == typed).then(|| {
            let count = self.take_count();
            let last = lines_down(text, offset, count.unwrap_or(1) - 1).max(offset);
            self.operate(operator, Span::Lines(offset, last), text)
        })
    }

    fn g_command(&mut self, typed: char, text: &str) -> Option<Step> {
        if typed == 'v' {
            return Some(self.reselect(text));
        }
        let operator = Operator::after_g(typed)?;
        self.operator = Some((operator, self.count.take()));
        Some(Step::Pending)
    }

    /// `gv`: the last Visual selection again, in its mode.
    fn reselect(&mut self, text: &str) -> Step {
        let Some((mode, anchor, head)) = self.last_visual else {
            return Step::Done(VimCommand::Nothing);
        };
        let fit = |offset: usize| Caret::at(offset).clamped(text).offset();
        self.mode = mode;
        self.anchor = fit(anchor);
        self.head = fit(head);
        Step::Done(VimCommand::Select(self.visual_caret(text)))
    }
}

/// The Trim level of the leader `t` with `count`.
fn trim_level(count: Option<usize>) -> TrimLevel {
    let last = TrimLevel::ALL.len() - 1;
    TrimLevel::ALL
        .get(count.unwrap_or(1).min(last))
        .copied()
        .unwrap_or(TrimLevel::SlightTrim)
}

/// Where `]s` and `[s` (Slots) or `]m` and `[m` (Lab marks) go from `offset`: the
/// start `times` starts after it, or before it. Past the last start, the last one. `None`
/// when no start lies that way, or `key` is no jump.
pub(super) fn jump_target(
    key: VimKey,
    text: &VimText<'_>,
    offset: usize,
    forward: bool,
    times: usize,
) -> Option<usize> {
    let ranges = match key {
        VimKey::Char('s') => &text.slots,
        VimKey::Char('m') => &text.lab_marks,
        VimKey::Char(_)
        | VimKey::Ctrl(_)
        | VimKey::Escape
        | VimKey::Enter
        | VimKey::Backspace
        | VimKey::Other => return None,
    };
    jump(ranges, offset, forward, times)
}

fn jump(ranges: &[Range<usize>], offset: usize, forward: bool, times: usize) -> Option<usize> {
    let mut starts: Vec<usize> = ranges.iter().map(|range| range.start).collect();
    starts.sort_unstable();
    starts.dedup();
    let ahead: Vec<usize> = if forward {
        starts.into_iter().filter(|start| *start > offset).collect()
    } else {
        starts
            .into_iter()
            .rev()
            .filter(|start| *start < offset)
            .collect()
    };
    let index = times.min(ahead.len()).checked_sub(1)?;
    ahead.get(index).copied()
}
