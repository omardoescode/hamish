use gpui::{
    Context, Entity, Focusable, IntoElement, Render, Subscription, Window, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};
use ka_teb_core::Document;

use crate::actions::{Stash, ToggleChrome, ToggleOverflow};
use crate::overflow_panel::{self, OverflowBlockDrag, OverflowPanel};
use crate::save_open::Notice;
use crate::save_open::SaveOpen;
use crate::settings_modal::{self, SaveFailed, SettingsModal};
use crate::{Editor, Theme};

/// The root view: the text column, the Counter, and the Chrome.
pub struct Workspace {
    pub(crate) editor: Entity<Editor>,
    pub(crate) save_open: SaveOpen,
    overflow_panel: Entity<OverflowPanel>,
    chrome_visible: bool,
    overflow_open: bool,
    pub(crate) settings_modal: Entity<SettingsModal>,
    pub(crate) settings_open: bool,
    _editor_changes: Subscription,
    _settings_saves: Subscription,
}

impl Workspace {
    pub fn new(document: Document, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| Editor::new(document, cx));
        window.focus(&editor.focus_handle(cx));
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        let overflow_panel = cx.new(|cx| OverflowPanel::new(editor.clone(), cx));
        let settings_modal = cx.new(SettingsModal::new);
        let settings_saves =
            cx.subscribe(&settings_modal, |workspace, _, failed: &SaveFailed, cx| {
                workspace.show(Notice::SettingsNotSaved(failed.0.clone()), cx);
            });
        Self {
            editor,
            overflow_panel,
            save_open: SaveOpen::with_notice(settings_modal::take_start_notice(cx)),
            chrome_visible: false,
            overflow_open: false,
            settings_modal,
            settings_open: false,
            _editor_changes: editor_changes,
            _settings_saves: settings_saves,
        }
    }

    #[must_use]
    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    /// FR-1: hidden on open. FR-2: a click on the Counter toggles it.
    #[must_use]
    pub fn chrome_visible(&self) -> bool {
        self.chrome_visible
    }

    /// FR-43: the Overflow panel is open.
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

    /// FR-45: after a Stash, Overflow shows where the text went. The Document keeps the keyboard.
    fn show_stash(&mut self, _: &Stash, _: &mut Window, cx: &mut Context<Self>) {
        self.overflow_open = true;
        cx.notify();
    }

    /// FR-47: a block dragged from Overflow goes into the Document at the drop point.
    fn drop_block(&mut self, _: &OverflowBlockDrag, window: &mut Window, cx: &mut Context<Self>) {
        let caret = self.overflow_panel.read(cx).caret();
        let position = window.mouse_position();
        self.editor.update(cx, |editor, cx| {
            editor.drop_overflow_block(caret, position, cx);
        });
    }

    fn toggle_chrome(&mut self, _: &ToggleChrome, _: &mut Window, cx: &mut Context<Self>) {
        self.chrome_visible = !self.chrome_visible;
        cx.notify();
    }
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
            .children(self.notice_view(&theme, cx))
            .when(self.settings_open, |workspace| {
                workspace.child(self.settings_modal.clone())
            })
    }
}
