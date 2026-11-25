//! The root view: it holds the editor, the panels, the Lab, and the Chrome, and routes
//! actions between them.

use gpui::{
    Context, Entity, Focusable, IntoElement, Render, Subscription, Window, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};
use ka_teb_core::{Document, SlotId};

use crate::actions::{AiAlternatives, OpenAlternatives, Stash, ToggleChrome, ToggleOverflow};
use crate::alternatives_panel::PanelEvent;
use crate::editor::EditorEvent;
use crate::lab::{self, Lab};
use crate::overflow_panel::{self, OverflowBlockDrag, OverflowPanel};
use crate::save_open::Notice;
use crate::save_open::SaveOpen;
use crate::settings;
use crate::settings_modal::{self, SaveFailed, SettingsModal};
use crate::{AlternativesPanel, Editor, Theme};

/// The root view: the text column, the Counter, and the Chrome.
pub struct Workspace {
    pub(crate) editor: Entity<Editor>,
    pub(crate) save_open: SaveOpen,
    lab: Entity<Lab>,
    overflow_panel: Entity<OverflowPanel>,
    chrome_visible: bool,
    alternatives: Option<(Entity<AlternativesPanel>, Subscription)>,
    overflow_open: bool,
    pub(crate) settings_modal: Entity<SettingsModal>,
    pub(crate) settings_open: bool,
    _editor_changes: Subscription,
    _settings_saves: Subscription,
    _editor_events: Subscription,
    _lab_changes: Subscription,
}

impl Workspace {
    pub fn new(document: Document, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| Editor::new(document, cx));
        window.focus(&editor.focus_handle(cx));
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        let editor_events = cx.subscribe_in(
            &editor,
            window,
            |workspace, _, event: &EditorEvent, window, cx| match *event {
                EditorEvent::SlotClicked(slot) => workspace.show_slot(slot, window, cx),
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
            chrome_visible: false,
            alternatives: None,
            overflow_open: false,
            settings_modal,
            settings_open: false,
            _editor_changes: editor_changes,
            _settings_saves: settings_saves,
            _editor_events: editor_events,
            _lab_changes: lab_changes,
        }
    }

    #[must_use]
    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    #[must_use]
    pub fn lab(&self) -> &Entity<Lab> {
        &self.lab
    }

    /// Hidden on open; a click on the Counter toggles it.
    #[must_use]
    pub fn chrome_visible(&self) -> bool {
        self.chrome_visible
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
        window.focus(&panel.focus_handle(cx));
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
        window.focus(&panel.focus_handle(cx));
    }

    fn close_alternatives(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.alternatives = None;
        self.editor.update(cx, |editor, cx| {
            editor.set_spotlight(None, cx);
            editor.set_preview(None, cx);
        });
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
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
        window.focus(&focus);
        cx.notify();
    }

    /// After a Stash, Overflow shows where the text went. The Document keeps the keyboard.
    fn show_stash(&mut self, _: &Stash, _: &mut Window, cx: &mut Context<Self>) {
        self.overflow_open = true;
        cx.notify();
    }

    /// A block dragged from Overflow goes into the Document at the drop point.
    fn drop_block(&mut self, _: &OverflowBlockDrag, window: &mut Window, cx: &mut Context<Self>) {
        let caret = self.overflow_panel.read(cx).caret();
        let position = window.mouse_position();
        self.editor.update(cx, |editor, cx| {
            editor.drop_overflow_block(caret, position, cx);
        });
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
    }

    fn toggle_chrome(&mut self, _: &ToggleChrome, _: &mut Window, cx: &mut Context<Self>) {
        self.chrome_visible = !self.chrome_visible;
        cx.notify();
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

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let counter_label = self.editor.read(cx).counter_label();
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
            .font_family(theme.font_family.clone())
            .key_context("Workspace")
            .on_action(cx.listener(Self::toggle_chrome))
            .on_action(cx.listener(Self::open_alternatives))
            .on_action(cx.listener(Self::ai_alternatives))
            .on_action(cx.listener(Self::toggle_overflow))
            .on_action(cx.listener(Self::show_stash))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::close_settings))
            .child(
                div()
                    .size_full()
                    .on_drop(cx.listener(Self::drop_block))
                    .child(self.editor.clone()),
            )
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::open))
            .map(|workspace| self.route_lab_actions(workspace))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(theme.edge_fade_height)
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
            .when(self.overflow_open, |workspace| {
                workspace.child(self.overflow_panel.clone())
            })
            .when_some(self.alternatives_panel().cloned(), |workspace, panel| {
                workspace.child(panel)
            })
            .child(
                div()
                    .id("counter")
                    .absolute()
                    .top(px(22.))
                    .left(px(24.))
                    .text_size(theme.font_size_hint)
                    .text_color(theme.text_faint)
                    .cursor_pointer()
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleChrome), cx))
                    .child(counter_label),
            )
            .when(self.chrome_visible, |workspace| {
                workspace.child(
                    div()
                        .id("chrome")
                        .absolute()
                        .bottom(px(24.))
                        .left(px(24.))
                        .right(px(24.))
                        .h(px(16.))
                        .flex()
                        .justify_between()
                        .child(self.save_open_icons(&theme, cx))
                        .child(
                            div()
                                .flex()
                                .gap(px(16.))
                                .child(settings_modal::settings_button(&theme))
                                .child(overflow_panel::overflow_button(self.overflow_open, &theme)),
                        ),
                )
            })
            .children(
                self.chrome_visible
                    .then(|| lab::lab_button(&self.lab, &theme, cx)),
            )
            .child(self.lab.clone())
            .children(self.notice_view(&theme, cx))
            .when(self.settings_open, |workspace| {
                workspace.child(self.settings_modal.clone())
            })
    }
}
