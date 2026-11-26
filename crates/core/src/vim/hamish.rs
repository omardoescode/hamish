//! The Hamish keys of Vim mode: the leader (Space), the `]` and `[` jumps and Slot cycle,
//! and `g` with Ghost, Revive, Stash and `gv`.

use std::ops::Range;

use super::motion::lines_down;
use super::operator::{Operator, Span};
use super::{Prefix, Step, Vim, VimCommand, VimKey, VimText};
use super::{char_of, select};
use crate::Caret;
use crate::{Cycle, Direction, TrimLevel};

/// An action of Hamish that a Vim mode key or a hamishrc map runs: the Editor runs the same
/// GPUI action as its key binding or menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimAction {
    /// Ask the AI for Variants of the target.
    AiAlternatives,
    /// Make the selection a Ghost, or Revive the Ghost at the caret.
    ToggleGhost,
    /// Move the selection to the end of Overflow.
    Stash,
    /// Delete the Active variant of the Slot at the caret.
    DeleteActiveVariant,
    /// Open or close Overflow.
    ToggleOverflow,
    /// Save the Document to its path; the first save asks for one.
    Save,
    /// Ask for a path, then save the Document there.
    SaveAs,
    /// Ask for a `.md` file, then open it.
    Open,
    /// Show an empty Document.
    NewDocument,
    /// Close Hamish, as `:q`: with unsaved changes, refuse with a Notice.
    Quit,
    /// Close Hamish and drop unsaved changes, as `:q!`.
    ForceQuit,
    /// Open Settings.
    OpenSettings,
    /// Open the Shortcuts sheet.
    OpenShortcuts,
    /// Turn the Markers off or on.
    ToggleMarkers,
    /// Turn the Markdown styles off or on.
    ToggleMarkdown,
    /// Read the hamishrc again.
    ReloadHamishrc,
    /// Open the hamishrc in the system editor.
    EditHamishrc,
    /// One Undo step back.
    Undo,
    /// One Undo step forward again.
    Redo,
    /// Copy the selection to the system clipboard.
    Copy,
    /// Move the selection to the system clipboard.
    Cut,
    /// Put the system clipboard text in at the caret.
    Paste,
    /// Select all the text.
    SelectAll,
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
    /// Delete the cuts still in the Trim proposal.
    MakeTrimCuts,
    /// Close the Trim bar; the text stays.
    CloseTrim,
    /// Put a copy of the Overflow block at the Overflow caret in at the Editor caret.
    UseOverflowBlock,
    /// Give the keyboard to the Editor.
    FocusEditor,
    /// Give the keyboard to Overflow, when it is open.
    FocusOverflow,
    /// Open the Command line and give it the keyboard.
    OpenCommandLine,
}

/// The actions that `:Name` in a hamishrc map runs, by name. `RunTrim1` to `RunTrim4` run
/// Slight trim to Cut in half, as the count of the leader `t`.
const NAMED_ACTIONS: [(&str, VimAction); 38] = [
    ("OpenAlternatives", VimAction::OpenAlternatives),
    ("AiAlternatives", VimAction::AiAlternatives),
    ("ToggleGhost", VimAction::ToggleGhost),
    ("Stash", VimAction::Stash),
    ("DeleteActiveVariant", VimAction::DeleteActiveVariant),
    ("UseOverflowBlock", VimAction::UseOverflowBlock),
    ("ToggleOverflow", VimAction::ToggleOverflow),
    ("FocusEditor", VimAction::FocusEditor),
    ("FocusOverflow", VimAction::FocusOverflow),
    ("ToggleLab", VimAction::ToggleLab),
    ("AcceptLabMark", VimAction::AcceptLabMark),
    ("DismissLabMark", VimAction::DismissLabMark),
    ("RunTrim1", VimAction::RunTrim(TrimLevel::SlightTrim)),
    ("RunTrim2", VimAction::RunTrim(TrimLevel::TightenMore)),
    ("RunTrim3", VimAction::RunTrim(TrimLevel::EvenSharper)),
    ("RunTrim4", VimAction::RunTrim(TrimLevel::CutInHalf)),
    ("KeepTrimCut", VimAction::KeepTrimCut),
    ("MakeTrimCuts", VimAction::MakeTrimCuts),
    ("CloseTrim", VimAction::CloseTrim),
    ("Save", VimAction::Save),
    ("SaveAs", VimAction::SaveAs),
    ("Open", VimAction::Open),
    ("NewDocument", VimAction::NewDocument),
    ("Quit", VimAction::Quit),
    ("ForceQuit", VimAction::ForceQuit),
    ("OpenSettings", VimAction::OpenSettings),
    ("OpenShortcuts", VimAction::OpenShortcuts),
    ("OpenCommandLine", VimAction::OpenCommandLine),
    ("ToggleMarkers", VimAction::ToggleMarkers),
    ("ToggleMarkdown", VimAction::ToggleMarkdown),
    ("ReloadHamishrc", VimAction::ReloadHamishrc),
    ("EditHamishrc", VimAction::EditHamishrc),
    ("Undo", VimAction::Undo),
    ("Redo", VimAction::Redo),
    ("Copy", VimAction::Copy),
    ("Cut", VimAction::Cut),
    ("Paste", VimAction::Paste),
    ("SelectAll", VimAction::SelectAll),
];

impl VimAction {
    /// The action that `:name` names in a hamishrc map; `None` for another name.
    pub(crate) fn named(name: &str) -> Option<Self> {
        NAMED_ACTIONS
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, action)| *action)
    }
}

impl Vim {
    /// The command of `key` after the prefix `prefix` in Normal mode, when the prefix is
    /// a Hamish key. `None` leaves the key to the Vim motions and text objects.
    pub(super) fn hamish_prefix(
        &mut self,
        prefix: Prefix,
        key: VimKey,
        text: &VimText<'_>,
        offset: usize,
    ) -> Option<Step> {
        match prefix {
            Prefix::Leader => Some(self.leader(key)),
            Prefix::Window => Some(Step::Done(window_command(key))),
            Prefix::Bracket(direction) => Some(self.bracket(direction, key, text, offset)),
            Prefix::G => self.after_g(key, text.text, offset),
            Prefix::Inner | Prefix::Around => None,
        }
    }

    /// As [`Vim::hamish_prefix`], in Visual mode: the jumps move the head, and Ghost,
    /// Revive and Stash act on the selection.
    pub(super) fn visual_hamish_prefix(
        &mut self,
        prefix: Prefix,
        key: VimKey,
        text: &VimText<'_>,
        head: usize,
    ) -> Option<Step> {
        match prefix {
            Prefix::Bracket(direction) => Some(self.visual_jump(direction, key, text, head)),
            Prefix::G if key == VimKey::Char('v') => Some(Step::Done(VimCommand::Nothing)),
            Prefix::G => {
                let operator = char_of(key).and_then(Operator::after_g)?;
                Some(self.visual_operate(operator, text.text, head))
            }
            Prefix::Inner | Prefix::Around | Prefix::Leader | Prefix::Window => None,
        }
    }

    /// `]s`, `[s`, `]m` and `[m` in Visual mode move the head.
    fn visual_jump(
        &mut self,
        direction: Direction,
        key: VimKey,
        text: &VimText<'_>,
        head: usize,
    ) -> Step {
        let times = self.take_count().unwrap_or(1);
        let Some(target) = jump_target(key, text, head, direction, times) else {
            return Step::Done(VimCommand::Nothing);
        };
        self.head = target;
        Step::Done(VimCommand::Select(self.visual_caret(text.text)))
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
            'p' => VimAction::UseOverflowBlock,
            _ => return Step::Done(VimCommand::Nothing),
        };
        Step::Done(VimCommand::Run(action))
    }

    /// Escape with no command pending closes the Trim bar while a Trim proposal is open.
    pub(super) fn escape(&self, text: &VimText<'_>) -> VimCommand {
        let pending = self.typed.len() > 1;
        match text.trim_cuts {
            Some(_) if !pending => VimCommand::Run(VimAction::CloseTrim),
            Some(_) | None => VimCommand::Nothing,
        }
    }

    /// While a Trim proposal is open: `n` and `N` go to the next and the cut before,
    /// and Enter makes the cuts. The leader `k` keeps the cut at the caret. `None` with no proposal
    /// or for another key.
    pub(super) fn trim_walk(
        &mut self,
        key: VimKey,
        count: Option<usize>,
        text: &VimText<'_>,
        offset: usize,
    ) -> Option<Step> {
        let cuts = text.trim_cuts.as_ref()?;
        if key == VimKey::Enter {
            return Some(Step::Done(VimCommand::Run(VimAction::MakeTrimCuts)));
        }
        let direction = match char_of(key)? {
            'n' => Direction::Forward,
            'N' => Direction::Back,
            _ => return None,
        };
        let target = jump(cuts, offset, direction, count.unwrap_or(1));
        self.column = None;
        Some(Step::Done(target.map_or(VimCommand::Nothing, select)))
    }

    /// `]v` and `[v` cycle the Slot at the caret; `]s`, `[s`, `]m` and `[m` jump.
    fn bracket(
        &mut self,
        direction: Direction,
        key: VimKey,
        text: &VimText<'_>,
        offset: usize,
    ) -> Step {
        let times = self.take_count().unwrap_or(1);
        let cycle = match direction {
            Direction::Forward => Cycle::Next,
            Direction::Back => Cycle::Previous,
        };
        if key == VimKey::Char('v') {
            return Step::Done(VimCommand::CycleSlot { cycle, times });
        }
        let Some(target) = jump_target(key, text, offset, direction, times) else {
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

/// The key after `Ctrl+W`: `h` gives the keyboard to the Editor, `l` to Overflow.
fn window_command(key: VimKey) -> VimCommand {
    match char_of(key) {
        Some('h') => VimCommand::Run(VimAction::FocusEditor),
        Some('l') => VimCommand::Run(VimAction::FocusOverflow),
        Some(_) | None => VimCommand::Nothing,
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
fn jump_target(
    key: VimKey,
    text: &VimText<'_>,
    offset: usize,
    direction: Direction,
    times: usize,
) -> Option<usize> {
    let ranges = match char_of(key)? {
        's' => &text.slots,
        'm' => &text.lab_marks,
        _ => return None,
    };
    jump(ranges, offset, direction, times)
}

fn jump(
    ranges: &[Range<usize>],
    offset: usize,
    direction: Direction,
    times: usize,
) -> Option<usize> {
    let mut starts: Vec<usize> = ranges.iter().map(|range| range.start).collect();
    starts.sort_unstable();
    starts.dedup();
    let ahead: Vec<usize> = match direction {
        Direction::Forward => starts.into_iter().filter(|start| *start > offset).collect(),
        Direction::Back => starts
            .into_iter()
            .rev()
            .filter(|start| *start < offset)
            .collect(),
    };
    let index = times.min(ahead.len()).checked_sub(1)?;
    ahead.get(index).copied()
}
