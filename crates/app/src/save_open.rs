//! New, Save, and Open: file dialogs, the unsaved sign, and the notice that shows
//! errors and what an open dropped (ADR 0007). File I/O runs on the background executor.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, Context, IntoElement, PathPromptOptions, PromptLevel, SharedString, Window, div,
    prelude::*, px,
};
use hamish_core::{Document, Dropped, LabCheck, Level, LineError, Opened, StorageError};

use crate::actions::{NewDocument, Open, Quit, Save, SaveAs};
use crate::chrome::elevated_surface;
use crate::{Theme, Workspace};

/// What the writer must know after a save or an open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    SaveFailed(String),
    OpenFailed(String),
    /// The open could not put these items back on the Document text (ADR 0007).
    Dropped(Vec<Dropped>),
    /// The settings file did not load at start; the defaults are in use.
    SettingsNotLoaded(String),
    SettingsNotSaved(String),
    /// A command of the Command line did not run.
    CommandFailed(LineError),
}

/// Where the Document is saved, and the open notice.
#[derive(Debug, Default)]
pub(crate) struct SaveOpen {
    path: Option<PathBuf>,
    notice: Option<Notice>,
    saving: bool,
    /// A save asked for while one runs; it starts when that one ends, so two saves never
    /// write the same file at once.
    queued: Option<PathBuf>,
    /// Counts the Documents that replaced the first one. A save that ends after a New or an
    /// Open belongs to a Document that is gone, so it changes nothing.
    generation: u64,
    /// Hamish closes when the running save ends well (`:wq`).
    quit_after_save: bool,
}

impl SaveOpen {
    fn replaced(&self) -> Self {
        Self {
            generation: self.generation + 1,
            ..Self::default()
        }
    }

    pub(crate) fn with_notice(notice: Option<Notice>) -> Self {
        Self {
            notice,
            ..Self::default()
        }
    }
}

impl Workspace {
    /// The `.md` file of the Document; `None` until the first save or an open.
    #[must_use]
    pub fn document_path(&self) -> Option<&Path> {
        self.save_open.path.as_deref()
    }

    #[must_use]
    pub fn notice(&self) -> Option<&Notice> {
        self.save_open.notice.as_ref()
    }

    /// Ctrl+S: saves to the Document's path; the first save asks for one.
    pub(crate) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.save_open.path.clone() else {
            self.save_as(&SaveAs, window, cx);
            return;
        };
        self.save_to(path, cx);
    }

    /// Ctrl+Shift+S: asks for a path, then saves there.
    pub(crate) fn save_as(&mut self, _: &SaveAs, _: &mut Window, cx: &mut Context<Self>) {
        let (directory, name) = match &self.save_open.path {
            Some(path) => (
                path.parent().map(Path::to_owned),
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned()),
            ),
            None => (None, None),
        };
        let directory = directory.or_else(std::env::home_dir).unwrap_or_default();
        let name = name.unwrap_or_else(|| "Untitled.md".to_owned());
        let chosen = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn(async move |this, cx| {
            let chosen = chosen.await;
            this.update(cx, |workspace, cx| match chosen {
                Ok(Ok(Some(path))) => workspace.save_to(path, cx),
                Ok(Err(error)) => {
                    let message = format!("cannot show the save dialog: {error:#}");
                    workspace.show(Notice::SaveFailed(message), cx);
                }
                Ok(Ok(None)) | Err(_) => {}
            })
        })
        .detach();
    }

    /// Saves to `path`, then closes Hamish when the save ends well.
    pub(crate) fn save_and_quit(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.save_open.quit_after_save = true;
        self.save_to(path, cx);
    }

    pub(crate) fn save_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.save_open.saving {
            self.save_open.queued = Some(path);
            return;
        }
        self.save_open.saving = true;
        let (document, at) = self.editor.update(cx, |editor, _| editor.snapshot());
        let target = path.clone();
        let generation = self.save_open.generation;
        let saving = cx.background_spawn(async move { document.save(&target) });
        cx.spawn(async move |this, cx| {
            let result = saving.await;
            this.update(cx, |workspace, cx| {
                if workspace.save_open.generation == generation {
                    workspace.saved(path, at, result, cx);
                }
            })
        })
        .detach();
    }

    fn saved(
        &mut self,
        path: PathBuf,
        at: Option<u64>,
        result: Result<(), StorageError>,
        cx: &mut Context<Self>,
    ) {
        self.save_open.saving = false;
        if result.is_err() {
            self.save_open.quit_after_save = false;
        }
        match result {
            Ok(()) => {
                self.editor
                    .update(cx, |editor, cx| editor.mark_saved(at, cx));
                self.save_open.path = Some(path);
                self.save_open.notice = None;
            }
            Err(error) => self.save_open.notice = Some(Notice::SaveFailed(error.to_string())),
        }
        let queued = self.save_open.queued.take();
        let quit = queued.is_none() && self.save_open.quit_after_save;
        if let Some(queued) = queued {
            self.save_to(queued, cx);
        }
        if quit {
            cx.defer(|cx| cx.dispatch_action(&Quit));
        }
        cx.notify();
    }

    /// Resolves `true` when the Document has no unsaved changes, or the writer discards them.
    fn ask_discard(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = bool> + use<> {
        let answer = self.editor.read(cx).is_unsaved().then(|| {
            window.prompt(
                PromptLevel::Warning,
                "Discard unsaved changes?",
                None,
                &["Discard", "Cancel"],
                cx,
            )
        });
        async move {
            match answer {
                Some(answer) => answer.await == Ok(0),
                None => true,
            }
        }
    }

    /// Ctrl+N: asks to discard unsaved changes, then shows an empty Untitled Document with no
    /// path and no undo steps. The panels close as on app start.
    pub(crate) fn new_document(
        &mut self,
        _: &NewDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let discard = self.ask_discard(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if !discard.await {
                return Ok(());
            }
            this.update_in(cx, |workspace, window, cx| {
                workspace
                    .editor
                    .update(cx, |editor, cx| editor.load(Document::default(), cx));
                workspace.save_open = workspace.save_open.replaced();
                workspace.reset_panels(window, cx);
            })
        })
        .detach();
    }

    /// Ctrl+O: asks to discard unsaved changes, then for a `.md` file, then opens it.
    pub(crate) fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let discard = self.ask_discard(window, cx);
        cx.spawn(async move |this, cx| {
            if !discard.await {
                return Ok(());
            }
            let chosen = cx.update(|cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Open".into()),
                })
            });
            let chosen = chosen
                .await
                .map(|chosen| chosen.map(|paths| paths.and_then(|paths| paths.into_iter().next())));
            this.update(cx, |workspace, cx| match chosen {
                Ok(Ok(Some(path))) => workspace.open_path(path, cx),
                Ok(Err(error)) => {
                    let message = format!("cannot show the open dialog: {error:#}");
                    workspace.show(Notice::OpenFailed(message), cx);
                }
                Ok(Ok(None)) | Err(_) => {}
            })
        })
        .detach();
    }

    /// Opens the `.md` file `path` with its Hamish comment in place of the Document. Unsaved
    /// changes are lost: [`Open`] asks first.
    pub fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let source = path.clone();
        let opening = cx.background_spawn(async move { Document::open(&source) });
        cx.spawn(async move |this, cx| {
            let result = opening.await;
            this.update(cx, |workspace, cx| workspace.opened(path, result, cx))
        })
        .detach();
    }

    fn opened(
        &mut self,
        path: PathBuf,
        result: Result<Opened, StorageError>,
        cx: &mut Context<Self>,
    ) {
        let Opened { document, dropped } = match result {
            Ok(opened) => opened,
            Err(error) => {
                self.show(Notice::OpenFailed(error.to_string()), cx);
                return;
            }
        };
        self.editor
            .update(cx, |editor, cx| editor.load(document, cx));
        self.save_open = self.save_open.replaced();
        self.save_open.path = Some(path);
        self.save_open.notice = (!dropped.is_empty()).then_some(Notice::Dropped(dropped));
        cx.notify();
    }

    pub(crate) fn show(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.save_open.notice = Some(notice);
        cx.notify();
    }

    /// The New, Save, and Open icons, top left in the Chrome. Save is in the accent color
    /// while the Document has unsaved changes.
    pub(crate) fn save_open_icons(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let unsaved = self.editor.read(cx).is_unsaved();
        let save_color = if unsaved {
            theme.accent
        } else {
            theme.text_muted
        };
        div()
            .flex()
            .gap(px(16.))
            .text_size(theme.font_size_hint)
            .child(
                div()
                    .id("new")
                    .debug_selector(|| "new".to_owned())
                    .cursor_pointer()
                    .text_color(theme.text_muted)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(NewDocument), cx))
                    .child("New"),
            )
            .child(
                div()
                    .id("save")
                    .debug_selector(|| "save".to_owned())
                    .cursor_pointer()
                    .text_color(save_color)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Save), cx))
                    .child("Save"),
            )
            .child(
                div()
                    .id("open")
                    .cursor_pointer()
                    .text_color(theme.text_muted)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(Open), cx))
                    .child("Open"),
            )
    }

    /// The notice at the bottom left; a click closes it.
    pub(crate) fn notice_view(&self, theme: &Theme, cx: &Context<Self>) -> Option<AnyElement> {
        let lines = notice_lines(self.save_open.notice.as_ref()?);
        let close = cx.listener(|workspace: &mut Self, _: &gpui::ClickEvent, _, cx| {
            workspace.save_open.notice = None;
            cx.notify();
        });
        Some(
            elevated_surface(theme)
                .id("notice")
                .absolute()
                .bottom(px(16.))
                .left(px(24.))
                .max_w(theme.column_width)
                .p(px(10.))
                .text_size(theme.font_size_hint)
                .text_color(theme.text)
                .cursor_pointer()
                .on_click(close)
                .children(lines.into_iter().map(|line| div().child(line)))
                .into_any_element(),
        )
    }
}

fn notice_lines(notice: &Notice) -> Vec<SharedString> {
    match notice {
        Notice::SaveFailed(error) => vec![format!("Not saved: {error}").into()],
        Notice::OpenFailed(error) => vec![format!("Not opened: {error}").into()],
        Notice::SettingsNotLoaded(error) => {
            vec![format!("Settings not loaded, defaults in use: {error}").into()]
        }
        Notice::SettingsNotSaved(error) => vec![format!("Settings not saved: {error}").into()],
        Notice::CommandFailed(error) => vec![error.to_string().into()],
        Notice::Dropped(dropped) => {
            std::iter::once("The .md file changed outside Hamish. Not found again:".into())
                .chain(dropped.iter().map(|item| dropped_line(item).into()))
                .collect()
        }
    }
}

fn dropped_line(dropped: &Dropped) -> String {
    match dropped {
        Dropped::Slot { level, text } => format!("{} Slot \"{text}\"", level_name(*level)),
        Dropped::Ghost { text } => format!("Ghost \"{text}\""),
        Dropped::LabMark { check, text } => format!("{} mark \"{text}\"", check_name(*check)),
    }
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Word => "Word",
        Level::Sentence => "Sentence",
        Level::Paragraph => "Paragraph",
    }
}

fn check_name(check: LabCheck) -> &'static str {
    match check {
        LabCheck::PunctuationTypos => "Punctuation and typos",
        LabCheck::WeakestSentences => "Weakest sentences",
        LabCheck::LongSentences => "Long sentences",
        LabCheck::ConvolutedSentences => "Convoluted sentences",
        LabCheck::OffToneWords => "Off-tone words",
        LabCheck::HedgesFiller => "Hedges and filler",
    }
}

#[cfg(test)]
mod tests {
    use hamish_core::{Dropped, LabCheck, Level};

    use super::{Notice, notice_lines};

    #[test]
    fn a_dropped_notice_names_each_item_with_its_text() {
        let notice = Notice::Dropped(vec![
            Dropped::Slot {
                level: Level::Sentence,
                text: "It rains.".to_owned(),
            },
            Dropped::Ghost {
                text: "maybe".to_owned(),
            },
            Dropped::LabMark {
                check: LabCheck::HedgesFiller,
                text: "really".to_owned(),
            },
        ]);

        assert_eq!(
            notice_lines(&notice),
            [
                "The .md file changed outside Hamish. Not found again:",
                "Sentence Slot \"It rains.\"",
                "Ghost \"maybe\"",
                "Hedges and filler mark \"really\"",
            ]
        );
    }
}
