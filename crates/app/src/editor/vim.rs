//! Vim mode in the Editor: keys go to the core `Vim` state machine and its commands run
//! here (ADR 0008). The register is the system clipboard.

use gpui::{Action, App, ClipboardItem, Context, KeyDownEvent, Window};
use hamish_core::{Caret, Document, VimAction, VimCommand, VimKey};

use super::Editor;
use crate::actions::OpenAlternatives;
use crate::caret::CaretView;
use crate::settings;
use crate::{AcceptLabMark, DismissLabMark, KeepTrimCut, RunTrim, ToggleLab};

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
        let command = self
            .vim
            .key(key, self.session.document(), self.session.caret());
        if command == VimCommand::PassOn {
            return false;
        }
        self.run_vim_command(command, window, cx);
        cx.notify();
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
        match command {
            VimCommand::PassOn | VimCommand::Nothing => {}
            VimCommand::Select(caret) => CaretView::select(self, caret, cx),
            VimCommand::Edit(edit) => CaretView::edit(self, &edit, cx),
            VimCommand::Cut { edit, removed } => {
                to_register(removed, cx);
                CaretView::edit(self, &edit, cx);
            }
            VimCommand::Yank { text, caret } => {
                to_register(text, cx);
                CaretView::select(self, caret, cx);
            }
            VimCommand::Put(put) => {
                let register = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .unwrap_or_default();
                let command = put.command(&register, self.session.document().text());
                self.run_vim_command(command, window, cx);
            }
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

pub(super) fn vim_on(cx: &App) -> bool {
    settings::is_on(cx, |settings| settings.vim_mode)
}

/// Puts `text` in the register, the system clipboard. Empty text leaves it as it is.
fn to_register(text: String, cx: &mut App) {
    if !text.is_empty() {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

/// The GPUI action of `action`: the one its key binding or menu item dispatches.
fn gpui_action(action: VimAction) -> Box<dyn Action> {
    match action {
        VimAction::OpenAlternatives => Box::new(OpenAlternatives),
        VimAction::ToggleLab => Box::new(ToggleLab),
        VimAction::RunTrim(level) => Box::new(RunTrim(level)),
        VimAction::AcceptLabMark => Box::new(AcceptLabMark),
        VimAction::DismissLabMark => Box::new(DismissLabMark),
        VimAction::KeepTrimCut => Box::new(KeepTrimCut),
    }
}

/// `keystroke` as Vim mode reads it: the char it types, or a named key.
fn vim_key(keystroke: &gpui::Keystroke) -> VimKey {
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
