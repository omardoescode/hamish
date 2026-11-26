//! Vim mode in the Editor: keys go to the core `Vim` state machine and its commands run
//! here (ADR 0008). The register is the system clipboard.

use gpui::{Action, App, ClipboardItem, Context, KeyDownEvent, Window};
use hamish_core::{
    Caret, Document, Fed, Motion, Vim, VimAction, VimCommand, VimKey, VimMode, one_char,
};

use super::Editor;
use crate::actions::{
    AiAlternatives, Copy, Cut, DeleteActiveVariant, EditHamishrc, FocusEditor, FocusOverflow,
    NewDocument, Open, OpenAlternatives, OpenCommandLine, OpenSettings, OpenShortcuts, Paste, Quit,
    QuitIfSaved, RecursiveMap, Redo, ReloadHamishrc, Save, SaveAs, SelectAll, Stash, ToggleGhost,
    ToggleMarkdown, ToggleMarkers, ToggleOverflow, Undo, UseOverflowBlock,
};
use crate::caret::{self, CaretView};
use crate::{
    AcceptLabMark, CloseTrim, DismissLabMark, KeepTrimCut, MakeTrimCuts, RunTrim, ToggleLab,
};
use crate::{hamishrc, settings};

/// A view with a `Vim` of its own: the Editor, or Overflow.
pub(crate) trait VimView: CaretView {
    fn vim(&self) -> &Vim;

    fn vim_mut(&mut self) -> &mut Vim;

    /// Gives `key` to the `Vim` of the view, with the text and the caret of the view.
    fn vim_command(&mut self, key: VimKey, cx: &mut Context<Self>) -> VimCommand;

    /// Runs `command`, a command other than `PassOn`. `was` is the Vim mode before its key.
    fn run_vim_command(
        &mut self,
        command: VimCommand,
        was: VimMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
}

/// A key that no key binding took goes through the hamishrc maps to Vim mode. A key Vim mode
/// uses goes no further, so in Normal mode no key reaches the text input. A key that a map
/// takes or holds goes no further either: the view types the keys of the map itself. A key
/// that no map can hold first flushes the held keys, then goes on as a key with none held.
pub(crate) fn vim_key_down<V: VimView>(
    view: &mut V,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if !vim_on(cx) {
        return;
    }
    let key = vim_key(&event.keystroke);
    if key == VimKey::Other {
        flush_held(view, window, cx);
    }
    let rc = hamishrc::current(cx);
    let fresh = view.vim().feed_is_empty();
    view.vim_mut().press(key);
    let mut next = view.vim_mut().next_key(&rc);
    let unmapped = fresh && next == Some(Fed::Key(key)) && view.vim().feed_is_empty();
    if unmapped && !run_vim(view, key, window, cx) {
        return;
    }
    cx.stop_propagation();
    if unmapped {
        return;
    }
    while let Some(fed) = next {
        run_fed(view, fed, window, cx);
        next = view.vim_mut().next_key(&rc);
    }
}

/// Runs the keys that the key feed holds for a longer map, with no wait for it. Call before a
/// key that does not reach the maps, such as Escape.
pub(crate) fn flush_held<V: VimView>(view: &mut V, window: &mut Window, cx: &mut Context<V>) {
    let rc = hamishrc::current(cx);
    while let Some(fed) = view.vim_mut().flush_key(&rc) {
        run_fed(view, fed, window, cx);
    }
}

/// Runs one thing the key feed gives: a key for Vim mode, a named action, or the Notice of a
/// recursive map.
fn run_fed<V: VimView>(view: &mut V, fed: Fed, window: &mut Window, cx: &mut Context<V>) {
    match fed {
        Fed::Key(key) => run_mapped_key(view, key, window, cx),
        Fed::Action(action) => window.dispatch_action(gpui_action(action), cx),
        Fed::TooDeep => window.dispatch_action(Box::new(RecursiveMap), cx),
    }
}

/// A key of a map: a key that Vim mode passes on goes in as the text input would type it.
fn run_mapped_key<V: VimView>(view: &mut V, key: VimKey, window: &mut Window, cx: &mut Context<V>) {
    if run_vim(view, key, window, cx) || !view.takes_text(cx) {
        return;
    }
    let mut utf8 = [0; 4];
    let typed = match key {
        VimKey::Char(typed) => typed.encode_utf8(&mut utf8),
        VimKey::Enter => "\n",
        VimKey::Backspace => {
            let edit = view.read_text(cx, |text, caret| caret.delete(text, Motion::Left));
            view.edit(&edit, cx);
            return;
        }
        VimKey::Ctrl(_) | VimKey::Escape | VimKey::Other => return,
    };
    let edit = view.read_text(cx, |text, caret| caret.input(text, None, typed));
    view.edit(&edit, cx);
    cx.notify();
}

/// Gives `key` to Vim mode and runs its command. `false` when Vim mode passes the key on.
pub(crate) fn run_vim<V: VimView>(
    view: &mut V,
    key: VimKey,
    window: &mut Window,
    cx: &mut Context<V>,
) -> bool {
    let was = view.vim().mode();
    let command = view.vim_command(key, cx);
    if command == VimCommand::PassOn {
        return false;
    }
    view.run_vim_command(command, was, window, cx);
    cx.notify();
    true
}

impl VimView for Editor {
    fn vim(&self) -> &Vim {
        &self.vim
    }

    fn vim_mut(&mut self) -> &mut Vim {
        &mut self.vim
    }

    fn vim_command(&mut self, key: VimKey, _: &mut Context<Self>) -> VimCommand {
        self.vim
            .key(key, self.session.document(), self.session.caret())
    }

    fn run_vim_command(
        &mut self,
        command: VimCommand,
        was: VimMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_command(command, window, cx);
        self.follow_vim_mode(was, self.vim.mode());
    }
}

impl Editor {
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
    fn run_command(&mut self, command: VimCommand, window: &mut Window, cx: &mut Context<Self>) {
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

/// The GPUI action of `action`: the one its key binding or menu item dispatches. The one
/// table of Hamish actions in the app (ADR 0008).
pub(crate) fn gpui_action(action: VimAction) -> Box<dyn Action> {
    match action {
        VimAction::AiAlternatives => Box::new(AiAlternatives),
        VimAction::ToggleGhost => Box::new(ToggleGhost),
        VimAction::Stash => Box::new(Stash),
        VimAction::DeleteActiveVariant => Box::new(DeleteActiveVariant),
        VimAction::ToggleOverflow => Box::new(ToggleOverflow),
        VimAction::Save => Box::new(Save),
        VimAction::SaveAs => Box::new(SaveAs),
        VimAction::Open => Box::new(Open),
        VimAction::NewDocument => Box::new(NewDocument),
        VimAction::Quit => Box::new(QuitIfSaved),
        VimAction::ForceQuit => Box::new(Quit),
        VimAction::OpenSettings => Box::new(OpenSettings),
        VimAction::OpenShortcuts => Box::new(OpenShortcuts),
        VimAction::ToggleMarkers => Box::new(ToggleMarkers),
        VimAction::ToggleMarkdown => Box::new(ToggleMarkdown),
        VimAction::ReloadHamishrc => Box::new(ReloadHamishrc),
        VimAction::EditHamishrc => Box::new(EditHamishrc),
        VimAction::Undo => Box::new(Undo),
        VimAction::Redo => Box::new(Redo),
        VimAction::Copy => Box::new(Copy),
        VimAction::Cut => Box::new(Cut),
        VimAction::Paste => Box::new(Paste),
        VimAction::SelectAll => Box::new(SelectAll),
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
        VimAction::OpenCommandLine => Box::new(OpenCommandLine),
    }
}

/// A key press with no Alt, platform or function key, as Vim mode and the Command line read
/// it.
pub(crate) enum KeyPress<'a> {
    /// Ctrl with this key name.
    Ctrl(&'a str),
    /// A key with no Ctrl: its name (`escape`, `tab`, ...) and the char it types, if one.
    Plain { key: &'a str, typed: Option<char> },
}

/// `keystroke` as a [`KeyPress`]; `None` with Alt, a platform key or a function key.
pub(crate) fn key_press(keystroke: &gpui::Keystroke) -> Option<KeyPress<'_>> {
    let modifiers = keystroke.modifiers;
    if modifiers.alt || modifiers.platform || modifiers.function {
        return None;
    }
    let key = keystroke.key.as_str();
    if modifiers.control {
        return Some(KeyPress::Ctrl(key));
    }
    let typed = if key == "space" {
        Some(' ')
    } else {
        keystroke.key_char.as_deref().and_then(one_char)
    };
    Some(KeyPress::Plain { key, typed })
}

/// `keystroke` as Vim mode reads it: the char it types, or a named key.
pub(crate) fn vim_key(keystroke: &gpui::Keystroke) -> VimKey {
    let (key, typed) = match key_press(keystroke) {
        None => return VimKey::Other,
        Some(KeyPress::Ctrl(key)) => return one_char(key).map_or(VimKey::Other, VimKey::Ctrl),
        Some(KeyPress::Plain { key, typed }) => (key, typed),
    };
    match key {
        "escape" => VimKey::Escape,
        "enter" => VimKey::Enter,
        "backspace" => VimKey::Backspace,
        _ => typed.map_or(VimKey::Other, VimKey::Char),
    }
}
