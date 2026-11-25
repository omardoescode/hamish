use gpui::{
    Context, Entity, Focusable, IntoElement, Render, Subscription, Window, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};
use ka_teb_core::{Document, SlotId};

use crate::actions::{OpenAlternatives, ToggleChrome};
use crate::alternatives_panel::PanelEvent;
use crate::editor::EditorEvent;
use crate::{AlternativesPanel, Editor, Theme};

/// The root view: the text column, the Counter, and the Chrome.
pub struct Workspace {
    editor: Entity<Editor>,
    chrome_visible: bool,
    alternatives: Option<(Entity<AlternativesPanel>, Subscription)>,
    _editor_changes: Subscription,
    _editor_events: Subscription,
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
            },
        );
        Self {
            editor,
            chrome_visible: false,
            alternatives: None,
            _editor_changes: editor_changes,
            _editor_events: editor_events,
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

    /// The open Alternatives panel; `None` when it is closed.
    #[must_use]
    pub fn alternatives_panel(&self) -> Option<&Entity<AlternativesPanel>> {
        self.alternatives.as_ref().map(|(panel, _)| panel)
    }

    /// FR-19: opens the Alternatives panel on the editor target and focuses it.
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

    /// FR-21: a click on a Slot while the panel is open shows that Slot in the panel.
    fn show_slot(&mut self, slot: SlotId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.alternatives_panel() else {
            return;
        };
        panel.update(cx, |panel, cx| panel.show_slot(slot, cx));
        window.focus(&panel.focus_handle(cx));
    }

    fn close_alternatives(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.alternatives = None;
        self.editor
            .update(cx, |editor, cx| editor.set_spotlight(None, cx));
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
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
            .on_action(cx.listener(Self::open_alternatives))
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
            .when_some(self.alternatives_panel().cloned(), |workspace, panel| {
                workspace.child(panel)
            })
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
