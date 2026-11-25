//! Shared Chrome pieces: the elevated surface, the list row, the text button, and the icons of
//! the Chrome bar. Each panel, popover and menu builds on them, so all share one look.

use gpui::{Action, Div, Hsla, IntoElement, Pixels, Stateful, div, prelude::*, px};

use crate::Theme;
use crate::actions::{OpenShortcuts, ToggleMarkdown, ToggleMarkers};

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

/// A Chrome bar icon that turns a setting on or off: `icon` in the accent color over an
/// accent underline when `on`, muted with no underline when off.
fn toggle_icon(
    id: &'static str,
    action: impl Action + Clone,
    on: bool,
    theme: &Theme,
    icon: Div,
) -> Stateful<Div> {
    let underline = if on {
        theme.accent
    } else {
        theme.accent.opacity(0.)
    };
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex()
        .flex_col()
        .items_center()
        .gap(px(3.))
        .cursor_pointer()
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
        .child(icon)
        .child(div().h(px(1.)).w_full().bg(underline))
}

fn icon_color(on: bool, theme: &Theme) -> Hsla {
    if on { theme.accent } else { theme.text_muted }
}

/// The Variant dots icon: three dots. It shows or hides the Markers.
pub(crate) fn markers_button(on: bool, theme: &Theme) -> impl IntoElement {
    let color = icon_color(on, theme);
    let dot = || div().size(px(4.)).rounded_full().bg(color);
    let dots = div()
        .h(px(12.))
        .flex()
        .items_center()
        .gap(px(3.))
        .child(dot())
        .child(dot())
        .child(dot());
    toggle_icon("markers-button", ToggleMarkers, on, theme, dots)
}

/// The Markdown icon: "M" and a down arrow. It turns the Markdown styles on or off.
pub(crate) fn markdown_button(on: bool, theme: &Theme) -> impl IntoElement {
    let mark = div()
        .h(px(12.))
        .flex()
        .items_center()
        .text_size(theme.font_size_hint)
        .text_color(icon_color(on, theme))
        .child("M\u{2193}");
    toggle_icon("markdown-button", ToggleMarkdown, on, theme, mark)
}

/// The Shortcuts icon: a keyboard outline with two rows of keys and a space bar.
pub(crate) fn shortcuts_button(theme: &Theme) -> Stateful<Div> {
    let color = theme.text_muted;
    let key = |width: Pixels| div().w(width).h(px(2.)).bg(color);
    let row = |count: usize| {
        div()
            .flex()
            .gap(px(1.))
            .children((0..count).map(|_| key(px(2.))))
    };
    div()
        .id("shortcuts-button")
        .debug_selector(|| "shortcuts-button".to_owned())
        .cursor_pointer()
        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenShortcuts), cx))
        .w(px(18.))
        .h(px(12.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(1.))
        .border_1()
        .border_color(color)
        .rounded(px(2.))
        .child(row(5))
        .child(row(5))
        .child(key(px(8.)))
}
