//! Shared Chrome pieces: the elevated surface, the list row and the text button.
//! Each panel, popover and menu builds on them, so all share one look.

use gpui::{Div, Hsla, div, prelude::*};

use crate::Theme;

/// A surface over the text: the elevated background and corner radius.
pub(crate) fn elevated_surface(theme: &Theme) -> Div {
    div()
        .bg(theme.elevated_background)
        .rounded(theme.elevated_radius)
}

/// A popover: an elevated surface with a shadow.
pub(crate) fn popover_surface(theme: &Theme) -> Div {
    elevated_surface(theme).shadow_md()
}

/// A list row; a `selected` row has the row hover background.
pub(crate) fn selectable_row(theme: &Theme, selected: bool) -> Div {
    div()
        .rounded(theme.elevated_radius)
        .when(selected, |row| row.bg(theme.elevated_row_hover))
}

/// A row the writer clicks: the row hover background shows under the pointer.
pub(crate) fn clickable_row(theme: &Theme) -> Div {
    selectable_row(theme, false)
        .cursor_pointer()
        .hover(|row| row.bg(theme.elevated_row_hover))
}

/// A button with an `outline` border and the row hover background under the pointer.
pub(crate) fn text_button(theme: &Theme, outline: Hsla) -> Div {
    div()
        .border_1()
        .border_color(outline)
        .rounded(theme.elevated_radius)
        .cursor_pointer()
        .hover(|button| button.bg(theme.elevated_row_hover))
}
