//! The root view: it holds the editor, the panels, the Lab, and the Chrome, and routes
//! actions between them.

use gpui::{
    App, Context, DragMoveEvent, Entity, FocusHandle, Focusable, IntoElement, Pixels, Render,
    SharedString, Subscription, Window, canvas, div, linear_color_stop, linear_gradient,
    prelude::*, px,
};
use hamish_core::{Document, SlotId, Toggle, document_title};

use crate::actions::{
    AiAlternatives, CloseMenu, FocusEditor, FocusOverflow, OpenAlternatives, ToggleMarkdown,
    ToggleMarkers, ToggleOverflow, UseOverflowBlock,
};
use crate::alternatives_panel::PanelEvent;
use crate::chrome;
use crate::editor::EditorEvent;
use crate::lab::{self, Lab};
use crate::overflow_panel::{self, OverflowBlockDrag, OverflowPanel};
use crate::save_open::Notice;
use crate::save_open::SaveOpen;
use crate::settings;
use crate::settings_modal::{self, SaveFailed, SettingsModal};
use crate::{AlternativesPanel, Editor, Theme};

/// The root view: the text column, the Document title, the Counter, and the Chrome.
pub struct Workspace {
    pub(crate) editor: Entity<Editor>,
    pub(crate) save_open: SaveOpen,
    lab: Entity<Lab>,
    overflow_panel: Entity<OverflowPanel>,
    alternatives: Option<(Entity<AlternativesPanel>, Subscription)>,
    overflow_open: bool,
    pub(crate) settings_modal: Entity<SettingsModal>,
    pub(crate) settings_open: bool,
    pub(crate) shortcuts_open: bool,
    /// The Shortcuts sheet has this focus while it is open.
    pub(crate) shortcuts_focus: FocusHandle,
    /// The title last set on the window, so a frame sets it only when it changes.
    window_title: String,
    /// The font family of the Chrome bar as last painted.
    chrome_font: SharedString,
    _editor_changes: Subscription,
    _settings_saves: Subscription,
    _editor_events: Subscription,
    lab_changes: Subscription,
}

/// Height of the Chrome bar at UI scale 1.
const CHROME_BAR_HEIGHT: Pixels = px(40.);

/// Height of the Chrome bar at the window top, at the UI scale in use.
pub(crate) fn chrome_bar_height(cx: &gpui::App) -> Pixels {
    CHROME_BAR_HEIGHT * settings::ui_scale(cx)
}

impl Workspace {
    pub fn new(document: Document, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| Editor::new(document, cx));
        window.focus(&editor.focus_handle(cx), cx);
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        let editor_events = cx.subscribe_in(
            &editor,
            window,
            |workspace, _, event: &EditorEvent, window, cx| match *event {
                EditorEvent::SlotClicked(slot) => workspace.show_slot(slot, window, cx),
                EditorEvent::Stashed => workspace.show_stash(cx),
                EditorEvent::VariantDeleted(_) | EditorEvent::VariantRestored(_) => {}
            },
        );
        let overflow_panel = cx.new(|cx| OverflowPanel::new(editor.clone(), cx));
        let lab = cx.new(|cx| Lab::new(editor.clone(), cx));
        let lab_changes = cx.observe(&lab, |_, _, cx| cx.notify());
        let settings_modal = cx.new(SettingsModal::new);
        let settings_saves =
            cx.subscribe(&settings_modal, |workspace, _, failed: &SaveFailed, cx| {
                workspace.show(Notice::SettingsNotSaved(failed.0.clone()), cx);
            });
        Self {
            editor,
            overflow_panel,
            save_open: SaveOpen::with_notice(settings::take_start_notice(cx)),
            lab,
            alternatives: None,
            overflow_open: false,
            settings_modal,
            settings_open: false,
            shortcuts_open: false,
            shortcuts_focus: cx.focus_handle(),
            window_title: String::new(),
            chrome_font: SharedString::default(),
            _editor_changes: editor_changes,
            _settings_saves: settings_saves,
            _editor_events: editor_events,
            lab_changes,
        }
    }

    #[must_use]
    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    /// The font family of the Chrome bar as last painted.
    #[must_use]
    pub fn chrome_font(&self) -> &SharedString {
        &self.chrome_font
    }

    #[must_use]
    pub fn lab(&self) -> &Entity<Lab> {
        &self.lab
    }

    /// The Document title at the top center and on the window, with " •" while unsaved.
    #[must_use]
    pub fn title_label(&self, cx: &App) -> String {
        let editor = self.editor.read(cx);
        let title = document_title(self.document_path(), editor.document());
        if editor.is_unsaved() {
            format!("{title} •")
        } else {
            title
        }
    }

    /// The Vim mode of the Editor as the Chrome bar shows it; `None` when the Vim mode
    /// setting is off.
    #[must_use]
    pub fn vim_status(&self, cx: &App) -> Option<String> {
        self.editor.read(cx).vim_status(cx)
    }

    /// The open Alternatives panel; `None` when it is closed.
    #[must_use]
    pub fn alternatives_panel(&self) -> Option<&Entity<AlternativesPanel>> {
        self.alternatives.as_ref().map(|(panel, _)| panel)
    }

    /// Opens the Alternatives panel on the editor target and focuses it.
    fn open_alternatives(
        &mut self,
        _: &OpenAlternatives,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.editor.clone();
        let panel = cx.new(|cx| AlternativesPanel::new(editor, cx));
        let events = cx.subscribe_in(
            &panel,
            window,
            |workspace, _, event, window, cx| match event {
                PanelEvent::Close => workspace.close_alternatives(window, cx),
            },
        );
        window.focus(&panel.focus_handle(cx), cx);
        self.alternatives = Some((panel, events));
        cx.notify();
    }

    /// Asks the AI for Variants of the panel target. From the editor (key or context
    /// menu), it first opens the panel on the editor target.
    fn ai_alternatives(&mut self, _: &AiAlternatives, window: &mut Window, cx: &mut Context<Self>) {
        let from_panel = self
            .alternatives_panel()
            .is_some_and(|panel| panel.focus_handle(cx).is_focused(window));
        if !from_panel {
            self.open_alternatives(&OpenAlternatives, window, cx);
        }
        if let Some(panel) = self.alternatives_panel() {
            panel.update(cx, AlternativesPanel::request_ai);
        }
    }

    /// A click on a Slot while the panel is open shows that Slot in the panel.
    fn show_slot(&mut self, slot: SlotId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.alternatives_panel() else {
            return;
        };
        panel.update(cx, |panel, cx| panel.show_slot(slot, cx));
        window.focus(&panel.focus_handle(cx), cx);
    }

    fn close_alternatives(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.alternatives = None;
        self.editor.update(cx, |editor, cx| {
            editor.set_spotlight(None, cx);
            editor.set_preview(None, cx);
        });
        window.focus(&self.editor.focus_handle(cx), cx);
        cx.notify();
    }

    /// Esc in the editor with no context menu open closes the Alternatives panel.
    fn close_menu(&mut self, _: &CloseMenu, window: &mut Window, cx: &mut Context<Self>) {
        if self.alternatives.is_some() {
            self.close_alternatives(window, cx);
        }
    }

    /// The Overflow panel is open.
    #[must_use]
    pub fn overflow_open(&self) -> bool {
        self.overflow_open
    }

    #[must_use]
    pub fn overflow_panel(&self) -> &Entity<OverflowPanel> {
        &self.overflow_panel
    }

    /// Opening gives Overflow the keyboard; closing gives it back to the Document.
    fn toggle_overflow(&mut self, _: &ToggleOverflow, window: &mut Window, cx: &mut Context<Self>) {
        self.overflow_open = !self.overflow_open;
        let focus = if self.overflow_open {
            self.overflow_panel.focus_handle(cx)
        } else {
            self.editor.focus_handle(cx)
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    /// `Ctrl+W h` in Vim mode: the Document gets the keyboard.
    fn focus_editor(&mut self, _: &FocusEditor, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.editor.focus_handle(cx), cx);
        cx.notify();
    }

    /// `Ctrl+W l` in Vim mode: open Overflow gets the keyboard. A closed Overflow stays
    /// closed.
    fn focus_overflow(&mut self, _: &FocusOverflow, window: &mut Window, cx: &mut Context<Self>) {
        if self.overflow_open {
            window.focus(&self.overflow_panel.focus_handle(cx), cx);
            cx.notify();
        }
    }

    /// The leader `p` in the Document: a copy of the Overflow block at the Overflow caret
    /// goes in at the Document caret.
    fn use_overflow_block(&mut self, _: &UseOverflowBlock, _: &mut Window, cx: &mut Context<Self>) {
        let caret = self.overflow_panel.read(cx).caret(cx);
        self.editor.update(cx, |editor, cx| {
            editor.use_overflow_block(caret, cx);
        });
    }

    /// The Variant dots and Markdown icons: flip `toggle` and save it. A failed save shows a
    /// notice.
    fn flip_setting(toggle: Toggle, cx: &mut Context<Self>) {
        let workspace = cx.weak_entity();
        settings::change(
            cx,
            |settings| settings.flip(toggle),
            move |error, cx| {
                let notice = Notice::SettingsNotSaved(error.to_string());
                workspace
                    .update(cx, |workspace, cx| workspace.show(notice, cx))
                    .ok();
            },
        );
    }

    /// After a Stash, Overflow shows where the text went. The Document keeps the keyboard.
    fn show_stash(&mut self, cx: &mut Context<Self>) {
        self.overflow_open = true;
        cx.notify();
    }

    /// A block dragged from Overflow goes into the Document at the drop point.
    fn drop_block(&mut self, _: &OverflowBlockDrag, window: &mut Window, cx: &mut Context<Self>) {
        let caret = self.overflow_panel.read(cx).caret(cx);
        let position = window.mouse_position();
        self.editor.update(cx, |editor, cx| {
            editor.drop_overflow_block(caret, position, cx);
        });
    }

    /// A block dragged over the page shows where it goes in.
    fn drag_block_over(
        &mut self,
        event: &DragMoveEvent<OverflowBlockDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = event.event.position;
        let over = event.bounds.contains(&position).then_some(position);
        self.editor
            .update(cx, |editor, cx| editor.drag_block_over(over, cx));
    }

    /// Closes the panels and starts a new Lab and Overflow panel, as on app start. The
    /// Document gets the keyboard.
    pub(crate) fn reset_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.alternatives = None;
        self.overflow_open = false;
        let editor = self.editor.clone();
        self.overflow_panel = cx.new(|cx| OverflowPanel::new(editor.clone(), cx));
        self.lab = cx.new(|cx| Lab::new(editor, cx));
        self.lab_changes = cx.observe(&self.lab, |_, _, cx| cx.notify());
        window.focus(&self.editor.focus_handle(cx), cx);
        cx.notify();
    }

    /// Lab actions that reach the Workspace (the focus is outside the Lab) go to the Lab.
    fn route_lab_actions(&self, root: gpui::Div) -> gpui::Div {
        let lab = &self.lab;
        root.on_action(to_lab(lab, Lab::toggle))
            .on_action(to_lab(lab, Lab::run_trim))
            .on_action(to_lab(lab, Lab::make_trim_cuts))
            .on_action(to_lab(lab, Lab::walk_trim))
            .on_action(to_lab(lab, Lab::walk_keep))
            .on_action(to_lab(lab, Lab::walk_cut))
            .on_action(to_lab(lab, Lab::close_trim))
            .on_action(to_lab(lab, Lab::keep_trim_cut))
            .on_action(to_lab(lab, Lab::accept_lab_mark))
            .on_action(to_lab(lab, Lab::dismiss_lab_mark))
    }
}

/// An action listener that runs `handle` on `lab`.
fn to_lab<A: 'static>(
    lab: &Entity<Lab>,
    handle: fn(&mut Lab, &A, &mut Window, &mut Context<Lab>),
) -> impl Fn(&A, &mut Window, &mut gpui::App) + 'static {
    let lab = lab.clone();
    move |action, window, cx| lab.update(cx, |lab, cx| handle(lab, action, window, cx))
}

impl Workspace {
    /// The Chrome bar at the window top, with no fill: the Counter, New, Save, and Open
    /// left; the Variant dots, Markdown, and LAB center; the Vim mode, Settings, Shortcuts,
    /// and Overflow right.
    fn chrome_bar(
        &self,
        theme: &Theme,
        height: Pixels,
        vim_status: Option<String>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let counter_label = self.editor.read(cx).counter_label();
        let markers = settings::is_on(cx, |settings| settings.markers);
        let markdown = settings::is_on(cx, |settings| settings.markdown);
        div()
            .id("chrome")
            .debug_selector(|| "chrome-bar".to_owned())
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(height)
            .px(px(24.))
            .flex()
            .items_center()
            .justify_between()
            .child(chrome_font_probe(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(24.))
                    .child(counter_view(counter_label, theme))
                    .child(self.save_open_icons(theme, cx)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(16.))
                    .child(chrome::markers_button(markers, theme))
                    .child(chrome::markdown_button(markdown, theme))
                    .child(lab::lab_button(&self.lab, theme, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .children(vim_status.map(|status| {
                        div()
                            .id("vim-mode")
                            .debug_selector(|| "vim-mode".to_owned())
                            .text_size(theme.font_size_hint)
                            .text_color(theme.text_muted)
                            .child(status)
                    }))
                    .child(settings_modal::settings_button(theme))
                    .child(chrome::shortcuts_button(theme))
                    .child(overflow_panel::overflow_button(self.overflow_open, theme)),
            )
    }
}

/// An empty element that keeps the font family the Chrome bar paints with.
fn chrome_font_probe(cx: &Context<Workspace>) -> impl IntoElement {
    let workspace = cx.weak_entity();
    canvas(
        move |_, window, cx| {
            let font = window.text_style().font_family;
            workspace
                .update(cx, |workspace, _| workspace.chrome_font = font)
                .ok();
        },
        |_, (), _, _| {},
    )
    .absolute()
}

/// The Document title, small and centered just under the Chrome bar.
fn title_view(label: String, theme: &Theme, bar_height: Pixels) -> impl IntoElement {
    div()
        .absolute()
        .top(bar_height)
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .text_size(theme.font_size_hint)
        .text_color(theme.text_muted)
        .child(label)
}

/// The Counter, at the left of the Chrome bar.
fn counter_view(label: String, theme: &Theme) -> impl IntoElement {
    div()
        .id("counter")
        .debug_selector(|| "counter".to_owned())
        .text_size(theme.font_size_hint)
        .text_color(theme.text_muted)
        .child(label)
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let bar_height = chrome_bar_height(cx);
        let title_label = self.title_label(cx);
        let overflow_keys =
            self.overflow_open && self.overflow_panel.focus_handle(cx).is_focused(window);
        let vim_status = if overflow_keys {
            self.overflow_panel.read(cx).vim_status(cx)
        } else {
            self.vim_status(cx)
        };
        if title_label != self.window_title {
            window.set_window_title(&title_label);
            self.window_title.clone_from(&title_label);
        }
        let fade = |angle| {
            linear_gradient(
                angle,
                linear_color_stop(theme.background, 0.),
                linear_color_stop(theme.background.opacity(0.), 1.),
            )
        };
        div()
            .size_full()
            .relative()
            .bg(theme.background)
            .font_family(theme.ui_font.clone())
            .key_context("Workspace")
            .on_action(cx.listener(Self::open_alternatives))
            .on_action(cx.listener(Self::ai_alternatives))
            .on_action(cx.listener(Self::toggle_overflow))
            .on_action(cx.listener(Self::focus_editor))
            .on_action(cx.listener(Self::focus_overflow))
            .on_action(cx.listener(Self::use_overflow_block))
            .on_action(cx.listener(|_, _: &ToggleMarkers, _, cx| {
                Self::flip_setting(Toggle::Markers, cx);
            }))
            .on_action(cx.listener(|_, _: &ToggleMarkdown, _, cx| {
                Self::flip_setting(Toggle::Markdown, cx);
            }))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::close_settings))
            .on_action(cx.listener(Self::open_shortcuts))
            .on_action(cx.listener(Self::close_shortcuts))
            .on_action(cx.listener(Self::close_menu))
            .child(
                div()
                    .size_full()
                    .on_drag_move(cx.listener(Self::drag_block_over))
                    .on_drop(cx.listener(Self::drop_block))
                    .child(self.editor.clone()),
            )
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::new_document))
            .map(|workspace| self.route_lab_actions(workspace))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(bar_height + theme.edge_fade_height)
                    .bg(fade(180.)),
            )
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(theme.edge_fade_height)
                    .bg(fade(0.)),
            )
            .child(
                div()
                    .absolute()
                    .top(bar_height)
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .when(self.overflow_open, |panels| {
                        panels.child(self.overflow_panel.clone())
                    })
                    .children(self.alternatives_panel().cloned()),
            )
            .child(title_view(title_label, &theme, bar_height))
            .child(self.chrome_bar(&theme, bar_height, vim_status, cx))
            .child(self.lab.clone())
            .children(self.notice_view(&theme, cx))
            .when(self.settings_open, |workspace| {
                workspace.child(self.settings_modal.clone())
            })
            .when(self.shortcuts_open, |workspace| {
                workspace.child(self.shortcuts_sheet(&theme, cx))
            })
    }
}
