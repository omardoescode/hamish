//! The Overflow panel (FR-43 to FR-47): right side, a free text area over the Document's
//! Overflow.

use gpui::{IntoElement, Render, SharedString, Window, div, prelude::*, px};

use crate::Theme;
use crate::actions::ToggleOverflow;

/// The Chrome button that opens and closes Overflow (FR-43): an outline badge, accent when open.
pub(crate) fn overflow_button(open: bool, theme: &Theme) -> impl IntoElement {
    let outline = if open {
        theme.accent_outline
    } else {
        theme.text_muted
    };
    div()
        .id("overflow-button")
        .debug_selector(|| "overflow-button".to_owned())
        .ml_auto()
        .px(px(4.))
        .border_1()
        .border_color(outline)
        .rounded(px(3.))
        .text_size(theme.font_size_hint)
        .text_color(outline)
        .cursor_pointer()
        .tooltip(|_, cx| {
            cx.new(|_| Tooltip {
                text: "Ctrl+Shift+O".into(),
            })
            .into()
        })
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleOverflow), cx))
        .child("XYZ")
}

struct Tooltip {
    text: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        div()
            .px(px(6.))
            .py(px(2.))
            .bg(theme.elevated_background)
            .rounded(theme.elevated_radius)
            .text_size(theme.font_size_hint)
            .text_color(theme.text_muted)
            .child(self.text.clone())
    }
}
