use gpui::{
    Context, Entity, Focusable, IntoElement, Render, Subscription, Window, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};
use ka_teb_core::Document;

use crate::actions::ToggleChrome;
use crate::{Editor, Theme};

/// The root view: the text column, the Counter, and the Chrome.
pub struct Workspace {
    editor: Entity<Editor>,
    chrome_visible: bool,
    _editor_changes: Subscription,
}

impl Workspace {
    pub fn new(document: Document, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| Editor::new(document, cx));
        window.focus(&editor.focus_handle(cx));
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        Self {
            editor,
            chrome_visible: false,
            _editor_changes: editor_changes,
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

    fn toggle_chrome(&mut self, _: &ToggleChrome, _: &mut Window, cx: &mut Context<Self>) {
        self.chrome_visible = !self.chrome_visible;
        cx.notify();
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
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
            .font_family(theme.font_family)
            .on_action(cx.listener(Self::toggle_chrome))
            .child(self.editor.clone())
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
                        .justify_between(),
                )
            })
    }
}
