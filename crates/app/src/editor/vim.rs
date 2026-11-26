//! Vim mode in the Editor: keys go to the core `Vim` state machine and its commands run
//! here (ADR 0008). The register is the system clipboard.

use gpui::{Action, App, ClipboardItem, Context, KeyDownEvent, Window};
use hamish_core::{Caret, Document, VimAction, VimCommand, VimKey, VimMode};

use super::Editor;
use crate::actions::{FocusEditor, FocusOverflow, OpenAlternatives, UseOverflowBlock};
use crate::caret::{self, CaretView};
use crate::settings;
use crate::{
    AcceptLabMark, CloseTrim, DismissLabMark, KeepTrimCut, MakeTrimCuts, RunTrim, ToggleLab,
};

impl Editor {
    /// A key that no key binding took goes to Vim mode. A key Vim mode uses goes no further,
    /// so in Normal mode no key reaches the text input.
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if vim_on(cx) && self.run_vim(vim_key(&event.keystroke), window, cx) {
            cx.stop_propagation();
        }
    }

    /// Gives `key` to Vim mode and runs its command. `false` when Vim mode passes the key on.
    pub(super) fn run_vim(
        &mut self,
        key: VimKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let was = self.vim.mode();
        let command = self
            .vim
            .key(key, self.session.document(), self.session.caret());
        if command == VimCommand::PassOn {
            return false;
        }
        self.run_vim_command(command, window, cx);
        self.follow_vim_mode(was, self.vim.mode());
        cx.notify();
        true
    }

    /// One Insert mode session, from the key that starts it to Escape, is one Undo step.
    pub(crate) fn follow_vim_mode(&mut self, was: VimMode, now: VimMode) {
        match (was == VimMode::Insert, now == VimMode::Insert) {
            (false, true) => self.session.start_insert(),
            (true, false) => self.session.end_insert(),
            (true, true) | (false, false) => {}
        }
    }

    /// A press at `position` in Visual mode moves the end of the Visual selection there;
    /// `false` outside Visual mode.
    pub(super) fn press_visual(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        extend: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !vim_on(cx) {
            return false;
        }
        let offset = caret::offset_at(self, position, cx);
        let Some(caret) = self
            .vim
            .click(self.session.document().text(), offset, extend)
        else {
            return false;
        };
        CaretView::select(self, caret, cx);
        true
    }

    /// Runs a Vim mode command. The register is the system clipboard. Edits go through the
    /// same path as the edits of the text input; Hamish actions through the same path as
    /// their key bindings and menu items.
    fn run_vim_command(
        &mut self,
        command: VimCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = run_text_command(self, command, cx) else {
            return;
        };
        match command {
            VimCommand::PassOn
            | VimCommand::Nothing
            | VimCommand::Select(_)
            | VimCommand::Edit(_)
            | VimCommand::Cut { .. }
            | VimCommand::Yank { .. }
            | VimCommand::Put(_) => {}
            VimCommand::Undo(times) => {
                for _ in 0..times {
                    self.undo_step(cx);
                }
                self.collapse_selection(cx);
            }
            VimCommand::Redo(times) => {
                for _ in 0..times {
                    self.redo_step(cx);
                }
                self.collapse_selection(cx);
            }
            VimCommand::Ghost(range) => self.change_from(range, Document::ghost, cx),
            VimCommand::Revive(range) => self.change_from(range, Document::revive, cx),
            VimCommand::Stash(range) => {
                self.stash_range(range, cx);
            }
            VimCommand::CycleSlot { cycle, times } => {
                self.cycle_at_caret(cycle, times, cx);
            }
            VimCommand::Run(action) => window.dispatch_action(gpui_action(action), cx),
        }
    }

    /// Runs `change` over `range` as one Undo step; the caret goes to the range start.
    fn change_from<E>(
        &mut self,
        range: std::ops::Range<usize>,
        change: fn(&mut Document, std::ops::Range<usize>) -> Result<(), E>,
        cx: &mut Context<Self>,
    ) {
        let start = range.start;
        if self
            .change_document(|document| change(document, range), cx)
            .is_ok()
        {
            CaretView::select(self, Caret::at(start), cx);
        }
    }

    /// After a Vim mode undo or redo, the caret goes to the start of the selection.
    fn collapse_selection(&mut self, cx: &mut Context<Self>) {
        let start = self.selection().start;
        CaretView::select(self, Caret::at(start), cx);
    }
}

/// Runs the commands of `view` that act on its caret and text: select, edit, cut, yank and
/// put. The register is the system clipboard. Gives back each other command.
pub(crate) fn run_text_command<V: CaretView>(
    view: &mut V,
    command: VimCommand,
    cx: &mut Context<V>,
) -> Option<VimCommand> {
    match command {
        VimCommand::PassOn | VimCommand::Nothing => {}
        VimCommand::Select(caret) => view.select(caret, cx),
        VimCommand::Edit(edit) => view.edit(&edit, cx),
        VimCommand::Cut { edit, removed } => {
            to_register(removed, cx);
            view.edit(&edit, cx);
        }
        VimCommand::Yank { text, caret } => {
            to_register(text, cx);
            view.select(caret, cx);
        }
        VimCommand::Put(put) => {
            let register = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap_or_default();
            let command = view.read_text(cx, |text, _| put.command(&register, text));
            return run_text_command(view, command, cx);
        }
        other @ (VimCommand::Undo(_)
        | VimCommand::Redo(_)
        | VimCommand::Ghost(_)
        | VimCommand::Revive(_)
        | VimCommand::Stash(_)
        | VimCommand::CycleSlot { .. }
        | VimCommand::Run(_)) => return Some(other),
    }
    None
}

pub(crate) fn vim_on(cx: &App) -> bool {
    settings::is_on(cx, |settings| settings.vim_mode)
}

/// Puts `text` in the register, the system clipboard. Empty text leaves it as it is.
fn to_register(text: String, cx: &mut App) {
    if !text.is_empty() {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

/// The GPUI action of `action`: the one its key binding or menu item dispatches.
pub(crate) fn gpui_action(action: VimAction) -> Box<dyn Action> {
    match action {
        VimAction::OpenAlternatives => Box::new(OpenAlternatives),
        VimAction::ToggleLab => Box::new(ToggleLab),
        VimAction::RunTrim(level) => Box::new(RunTrim(level)),
        VimAction::AcceptLabMark => Box::new(AcceptLabMark),
        VimAction::DismissLabMark => Box::new(DismissLabMark),
        VimAction::KeepTrimCut => Box::new(KeepTrimCut),
        VimAction::MakeTrimCuts => Box::new(MakeTrimCuts),
        VimAction::CloseTrim => Box::new(CloseTrim),
        VimAction::UseOverflowBlock => Box::new(UseOverflowBlock),
        VimAction::FocusEditor => Box::new(FocusEditor),
        VimAction::FocusOverflow => Box::new(FocusOverflow),
    }
}

/// `keystroke` as Vim mode reads it: the char it types, or a named key.
pub(crate) fn vim_key(keystroke: &gpui::Keystroke) -> VimKey {
    let modifiers = keystroke.modifiers;
    if modifiers.alt || modifiers.platform || modifiers.function {
        return VimKey::Other;
    }
    if modifiers.control {
        return one_char(Some(keystroke.key.as_str())).map_or(VimKey::Other, VimKey::Ctrl);
    }
    match keystroke.key.as_str() {
        "escape" => VimKey::Escape,
        "enter" => VimKey::Enter,
        "backspace" => VimKey::Backspace,
        "space" => VimKey::Char(' '),
        _ => one_char(keystroke.key_char.as_deref()).map_or(VimKey::Other, VimKey::Char),
    }
}

/// The char of `typed` when it is exactly one char.
fn one_char(typed: Option<&str>) -> Option<char> {
    let mut chars = typed?.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}
