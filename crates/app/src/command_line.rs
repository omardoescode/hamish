//! The Command line of Vim mode: one line at the window bottom that `:` opens. Core reads
//! its keys and commands; here it draws, runs the commands, and lists directories for Tab on
//! the background executor.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, Context, FocusHandle, Focusable, KeyDownEvent, Keystroke, Window, div, prelude::*,
    px,
};
use hamish_core::{
    CommandLine, DirectoryEntry, LineCommand, LineError, LineKey, LineStep, resolve_path,
};

use crate::actions::{OpenCommandLine, OpenSettings, Save};
use crate::caret::{CARET_WIDTH, CaretBlink};
use crate::save_open::Notice;
use crate::{Theme, Workspace};

/// The Command line with its focus.
pub(crate) struct CommandLineBar {
    line: CommandLine,
    focus: FocusHandle,
    /// `None` while the Command line is closed.
    open: Option<OpenLine>,
}

/// The state of an open Command line.
struct OpenLine {
    /// The focus to give back on close.
    back: FocusHandle,
    blink: CaretBlink,
}

impl CommandLineBar {
    pub(crate) fn new(focus: FocusHandle) -> Self {
        Self {
            line: CommandLine::default(),
            focus,
            open: None,
        }
    }
}

impl Workspace {
    /// The text of the open Command line, after the `:`; `None` when it is closed.
    #[must_use]
    pub fn command_line(&self) -> Option<&str> {
        self.command_line
            .open
            .as_ref()
            .map(|_| self.command_line.line.text())
    }

    /// `:` in Normal mode: opens the Command line and gives it the keyboard. Closing it gives
    /// the keyboard back to the view that had it.
    pub(crate) fn open_command_line(
        &mut self,
        _: &OpenCommandLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let back = window
            .focused(cx)
            .unwrap_or_else(|| self.editor.focus_handle(cx));
        self.command_line.open = Some(OpenLine {
            back,
            blink: CaretBlink::new(cx),
        });
        window.focus(&self.command_line.focus, cx);
        cx.notify();
    }

    fn close_command_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self
            .command_line
            .open
            .take()
            .map_or_else(|| self.editor.focus_handle(cx), |open| open.back);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn command_line_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = line_key(&event.keystroke) else {
            return;
        };
        cx.stop_propagation();
        if let Some(open) = &self.command_line.open {
            open.blink.restart(cx);
        }
        match self.command_line.line.key(key) {
            LineStep::Open => cx.notify(),
            LineStep::Closed => self.close_command_line(window, cx),
            LineStep::Run(result) => {
                self.close_command_line(window, cx);
                self.run_line_result(result, window, cx);
            }
            LineStep::ListDirectory(typed) => self.list_directory(typed, cx),
        }
    }

    fn run_line_result(
        &mut self,
        result: Result<LineCommand, LineError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let unsaved = self.editor.read(cx).is_unsaved();
        let refused = result.and_then(|command| {
            command
                .refusal(unsaved, self.document_path().is_some())
                .map_or(Ok(command), Err)
        });
        match refused {
            Ok(command) => self.run_line_command(command, unsaved, window, cx),
            Err(error) => self.show(Notice::CommandFailed(error), cx),
        }
    }

    fn run_line_command(
        &mut self,
        command: LineCommand,
        unsaved: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            LineCommand::Write(None) => self.save(&Save, window, cx),
            LineCommand::Write(Some(typed)) => self.save_to(self.typed_path(&typed), cx),
            LineCommand::Quit { .. }
            | LineCommand::WriteQuit {
                path: None,
                only_unsaved: true,
            } if !unsaved => cx.quit(),
            LineCommand::Quit { .. } => cx.quit(),
            LineCommand::WriteQuit { path, .. } => self.write_quit(path, cx),
            LineCommand::Edit { path, .. } => self.edit(path, cx),
            LineCommand::SetVimMode(on) => {
                Self::change_setting(move |settings| settings.vim_mode = on, cx);
            }
            LineCommand::ToggleChrome => {
                self.chrome_shown = !self.chrome_shown;
                cx.notify();
            }
            LineCommand::OpenSettings => self.open_settings(&OpenSettings, window, cx),
        }
    }

    /// `:wq` and `:x`: saves to `typed`, else to the Document path, then closes Hamish.
    fn write_quit(&mut self, typed: Option<String>, cx: &mut Context<Self>) {
        let Some(path) = self.command_path(typed) else {
            return;
        };
        self.save_and_quit(path, cx);
    }

    /// `:e`: opens `typed`, else the Document file again.
    fn edit(&mut self, typed: Option<String>, cx: &mut Context<Self>) {
        let Some(path) = self.command_path(typed) else {
            return;
        };
        self.open_path(path, cx);
    }

    /// The file path of `typed`, else the Document path.
    fn command_path(&self, typed: Option<String>) -> Option<PathBuf> {
        match typed {
            Some(typed) => Some(self.typed_path(&typed)),
            None => self.document_path().map(Path::to_owned),
        }
    }

    /// The file path of a path typed on the Command line.
    fn typed_path(&self, typed: &str) -> PathBuf {
        resolve_path(
            typed,
            std::env::home_dir().as_deref(),
            self.document_path().and_then(Path::parent),
        )
    }

    /// Lists the directory `typed` on the background executor, then completes the path on
    /// the Command line. A directory that does not list gives no candidates.
    fn list_directory(&mut self, typed: String, cx: &mut Context<Self>) {
        let directory = self.typed_path(&typed);
        let listing = cx.background_spawn(async move { directory_entries(&directory) });
        cx.spawn(async move |this, cx| {
            let entries = listing.await;
            this.update(cx, |workspace, cx| {
                workspace.command_line.line.complete_paths(&typed, entries);
                cx.notify();
            })
        })
        .detach();
    }

    /// The Command line at the window bottom while it is open: `:`, the text, and the
    /// blinking caret.
    pub(crate) fn command_line_view(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.command_line.open.as_ref()?;
        let line = &self.command_line.line;
        let (before, after) = line
            .text()
            .split_at_checked(line.caret())
            .unwrap_or((line.text(), ""));
        let shown = open.blink.shown(window, cx);
        Some(
            div()
                .id("command-line")
                .debug_selector(|| "command-line".to_owned())
                .key_context("CommandLine")
                .track_focus(&self.command_line.focus)
                .on_key_down(cx.listener(Self::command_line_key))
                .occlude()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .flex()
                .items_center()
                .px(px(24.))
                .py(px(6.))
                .bg(theme.background)
                .text_size(theme.font_size_hint)
                .text_color(theme.text)
                .child(format!(":{before}"))
                .child(
                    div()
                        .debug_selector(|| "command-line-caret".to_owned())
                        .flex_none()
                        .w(CARET_WIDTH)
                        .h(theme.line_height_overflow)
                        .bg(theme.accent)
                        .opacity(if shown { 1. } else { 0. }),
                )
                .child(after.to_owned())
                .into_any_element(),
        )
    }
}

/// The entries of `directory`; none when it does not list.
fn directory_entries(directory: &Path) -> Vec<DirectoryEntry> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            Some(DirectoryEntry {
                name,
                directory: entry.path().is_dir(),
            })
        })
        .collect()
}

/// `keystroke` as the Command line reads it; `None` for a key it does not use.
fn line_key(keystroke: &Keystroke) -> Option<LineKey> {
    let modifiers = keystroke.modifiers;
    if modifiers.alt || modifiers.platform || modifiers.function {
        return None;
    }
    if modifiers.control {
        return ctrl_line_key(&keystroke.key);
    }
    let key = match keystroke.key.as_str() {
        "escape" => LineKey::Escape,
        "enter" => LineKey::Enter,
        "backspace" => LineKey::Backspace,
        "tab" => LineKey::Tab,
        "up" => LineKey::Up,
        "down" => LineKey::Down,
        "left" => LineKey::Left,
        "right" => LineKey::Right,
        "home" => LineKey::Home,
        "end" => LineKey::End,
        "space" => LineKey::Char(' '),
        _ => {
            let mut chars = keystroke.key_char.as_deref()?.chars();
            let typed = chars.next()?;
            return chars.next().is_none().then_some(LineKey::Char(typed));
        }
    };
    Some(key)
}

/// The Command line key of Ctrl with `key`.
fn ctrl_line_key(key: &str) -> Option<LineKey> {
    match key {
        "b" => Some(LineKey::Home),
        "e" => Some(LineKey::End),
        "w" => Some(LineKey::DeleteWord),
        "u" => Some(LineKey::DeleteToStart),
        _ => None,
    }
}
