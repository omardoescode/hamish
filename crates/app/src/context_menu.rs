//! The right-click context menu of the editor.

use gpui::{
    Action, App, Corner, IntoElement, MouseDownEvent, Pixels, Point, Window, anchored, deferred,
    div, prelude::*, px,
};

use crate::Theme;
use crate::actions::{AiAlternatives, OpenAlternatives, Stash, ToggleGhost};

/// One row of the context menu: its label and its shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuItem {
    pub label: String,
    pub shortcut: &'static str,
}

/// The open context menu: where the pointer opened it and what its rows act on.
pub(crate) struct ContextMenu {
    pub(crate) position: Point<Pixels>,
    /// The word a right-click with no selection acts on.
    pub(crate) word: Option<String>,
    /// The target touches a Ghost range: "Revive it" in place of "Ghost it".
    pub(crate) in_ghost: bool,
}

impl ContextMenu {
    pub(crate) fn items(&self) -> Vec<ContextMenuItem> {
        let subject = self
            .word
            .as_ref()
            .map_or_else(|| "selection".to_owned(), |word| format!("\"{word}\""));
        let ghost = if self.in_ghost {
            "Revive it"
        } else {
            "Ghost it"
        };
        vec![
            ContextMenuItem {
                label: format!("Alternatives for {subject}"),
                shortcut: "Ctrl+Shift+A",
            },
            ContextMenuItem {
                label: format!("AI alternatives for {subject}"),
                shortcut: "Ctrl+Shift+G",
            },
            ContextMenuItem {
                label: ghost.to_owned(),
                shortcut: "Ctrl+/",
            },
            ContextMenuItem {
                label: "Stash this in Overflow".to_owned(),
                shortcut: "Ctrl+Shift+X",
            },
        ]
    }

    /// The menu overlay at the pointer. A row dispatches its action to the focused editor;
    /// a mouse down outside the menu calls `dismiss`.
    pub(crate) fn render(
        &self,
        theme: &Theme,
        dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let actions: [Box<dyn Action>; 4] = [
            Box::new(OpenAlternatives),
            Box::new(AiAlternatives),
            Box::new(ToggleGhost),
            Box::new(Stash),
        ];
        let rows =
            self.items()
                .into_iter()
                .zip(actions)
                .enumerate()
                .map(|(index, (item, action))| {
                    div()
                        .id(("context-menu-row", index))
                        .debug_selector(|| format!("context-menu-row-{index}"))
                        .flex()
                        .justify_between()
                        .gap(px(24.))
                        .px(px(12.))
                        .rounded(theme.elevated_radius)
                        .cursor_pointer()
                        .hover(|row| row.bg(theme.elevated_row_hover))
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(action.boxed_clone(), cx);
                        })
                        .child(div().text_color(theme.text).child(item.label))
                        .child(div().text_color(theme.text_muted).child(item.shortcut))
                });
        deferred(
            anchored()
                .position(self.position)
                .anchor(Corner::TopLeft)
                .snap_to_window()
                .child(
                    div()
                        .id("context-menu")
                        .occlude()
                        .p(px(4.))
                        .bg(theme.elevated_background)
                        .rounded(theme.elevated_radius)
                        .shadow_md()
                        .on_mouse_down_out(dismiss)
                        .children(rows),
                ),
        )
    }
}
