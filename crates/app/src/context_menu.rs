use std::ops::Range;

use gpui::{
    Action, App, Corner, IntoElement, MouseDownEvent, Pixels, Point, Window, anchored, deferred,
    div, prelude::*, px,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::Theme;
use crate::actions::{AiAlternatives, OpenAlternatives, Stash, ToggleGhost};

/// One row of the context menu: its label and its shortcut (FR-28 to FR-31).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuItem {
    pub label: String,
    pub shortcut: &'static str,
}

/// The open context menu: where the pointer opened it and what its rows act on.
pub(crate) struct ContextMenu {
    pub(crate) position: Point<Pixels>,
    /// The word a right-click with no selection acts on (FR-29).
    pub(crate) word: Option<String>,
    /// The target touches a Ghost range: "Revive it" in place of "Ghost it" (FR-30).
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

/// The word at `offset`: the run of word chars `offset` lies in or on an edge of.
pub(crate) fn word_at(text: &str, offset: usize) -> Option<Range<usize>> {
    text.split_word_bound_indices()
        .filter(|(_, word)| word.chars().any(char::is_alphanumeric))
        .map(|(start, word)| start..start + word.len())
        .find(|word| word.start <= offset && offset <= word.end)
}

/// `target` touches a Ghost range: it overlaps one, or as a caret lies in or on an edge of one.
pub(crate) fn touches_ghost(ghosts: &[Range<usize>], target: &Range<usize>) -> bool {
    ghosts.iter().any(|ghost| {
        let overlaps = ghost.start < target.end && target.start < ghost.end;
        let holds_caret =
            target.is_empty() && ghost.start <= target.start && target.start <= ghost.end;
        overlaps || holds_caret
    })
}

#[cfg(test)]
mod tests {
    use super::{touches_ghost, word_at};

    #[test]
    fn the_word_at_an_offset_includes_its_edges() {
        let text = "The customer waits.";
        assert_eq!(word_at(text, 7), Some(4..12));
        assert_eq!(word_at(text, 4), Some(4..12));
        assert_eq!(word_at(text, 12), Some(4..12));
        assert_eq!(word_at(text, 0), Some(0..3));
    }

    #[test]
    fn spaces_and_punctuation_are_no_word() {
        assert_eq!(word_at("a  . b", 2), None);
        assert_eq!(word_at("a  . b", 4), None);
    }

    #[test]
    fn a_caret_on_the_edge_of_a_ghost_touches_it_and_a_range_beside_it_does_not() {
        let ghosts = [4..12, 20..25];
        assert!(touches_ghost(&ghosts, &(12..12)));
        assert!(touches_ghost(&ghosts, &(10..14)));
        assert!(!touches_ghost(&ghosts, &(12..14)));
        assert!(!touches_ghost(&ghosts, &(0..3)));
        assert!(touches_ghost(&ghosts, &(24..30)));
    }
}
