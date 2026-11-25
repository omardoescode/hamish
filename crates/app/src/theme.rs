//! The Theme global: colors and sizes for each view, made from the Settings.

use gpui::{Global, Hsla, Pixels, SharedString, hsla, px, rgb};
use ka_teb_core::{Palette, Settings};

/// Theme values: the colors of the named theme in [`Settings`], and sizes from them.
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Hsla,
    pub panel_background: Hsla,
    pub panel_border: Hsla,
    /// Width of the Alternatives and Overflow panels.
    pub panel_width: Pixels,
    pub elevated_background: Hsla,
    pub elevated_row_hover: Hsla,
    /// Corner radius of popovers and the context menu.
    pub elevated_radius: Pixels,
    pub text: Hsla,
    pub text_title: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    /// Markdown headings `#` to `###`, bold.
    pub text_heading: Hsla,
    pub text_quote: Hsla,
    pub list_marker: Hsla,
    /// Translucent, so the selection under it still shows.
    pub code_background: Hsla,
    pub accent: Hsla,
    pub accent_outline: Hsla,
    pub slot_underline: Hsla,
    pub slot_dot: Hsla,
    pub slot_dot_filled: Hsla,
    pub slot_paragraph_line: Hsla,
    /// Background + about 4% white (dark theme) or black (light theme).
    pub slot_hover_background: Hsla,
    pub selection: Hsla,
    /// Background of Lab marks (mark style inferred).
    pub lab_mark: Hsla,
    pub ghost_opacity: f32,
    /// Text of the cuts of a Trim proposal.
    pub trim_fade_opacity: f32,
    /// Text outside the current paragraph while the Alternatives panel is open.
    pub panel_open_dim_opacity: f32,
    pub edge_fade_height: Pixels,
    /// Font family of the Chrome, panels, Settings, menus and popovers.
    pub ui_font: SharedString,
    /// Font family of the Document text and the Overflow text.
    pub text_font: SharedString,
    pub font_size_body: Pixels,
    /// Markdown heading sizes, `#` to `###`; they share the body line height.
    pub font_size_heading: [Pixels; 3],
    pub font_size_hint: Pixels,
    pub line_height_body: Pixels,
    pub font_size_overflow: Pixels,
    pub line_height_overflow: Pixels,
    pub column_width: Pixels,
    /// Space above the title when scrolled to the start.
    pub column_top: Pixels,
}

impl Theme {
    /// The theme for `settings`: its colors, fonts, and the Chrome at its UI scale.
    #[must_use]
    pub fn new(settings: &Settings) -> Self {
        let scale = settings.ui_scale;
        let base = Self::from_palette(settings.theme.palette());
        Self {
            panel_width: base.panel_width * scale,
            elevated_radius: base.elevated_radius * scale,
            font_size_hint: base.font_size_hint * scale,
            font_size_overflow: base.font_size_overflow * scale,
            line_height_overflow: base.line_height_overflow * scale,
            ui_font: settings.ui_font.clone().into(),
            text_font: settings.text_font_family().to_owned().into(),
            font_size_body: px(settings.font_size),
            font_size_heading: base
                .font_size_heading
                .map(|size| size * (settings.font_size / 15.)),
            line_height_body: px(settings.font_size * settings.line_height),
            column_width: px(settings.column_width),
            ..base
        }
    }

    fn from_palette(palette: Palette) -> Self {
        let color = |value: u32| -> Hsla { rgb(value).into() };
        let (wash, lab_alpha) = if palette.dark { (1., 0.18) } else { (0., 0.16) };
        Self {
            background: color(palette.background),
            panel_background: color(palette.panel_background),
            panel_border: color(palette.panel_border),
            panel_width: px(205.),
            elevated_background: color(palette.elevated_background),
            elevated_row_hover: color(palette.elevated_row_hover),
            elevated_radius: px(6.),
            text: color(palette.text),
            text_title: color(palette.text_title),
            text_muted: color(palette.text_muted),
            text_faint: color(palette.text_faint),
            text_heading: color(palette.text_title),
            text_quote: color(palette.text_quote),
            list_marker: color(palette.list_marker),
            code_background: hsla(0., 0., wash, 0.06),
            accent: color(palette.accent),
            accent_outline: color(palette.accent_outline),
            slot_underline: color(palette.slot_underline),
            slot_dot: color(palette.slot_underline),
            slot_dot_filled: color(palette.slot_dot_filled),
            slot_paragraph_line: color(palette.slot_paragraph_line),
            slot_hover_background: hsla(0., 0., wash, 0.04),
            selection: color(palette.selection),
            lab_mark: color(palette.lab_mark).opacity(lab_alpha),
            ghost_opacity: 0.10,
            trim_fade_opacity: 0.22,
            panel_open_dim_opacity: 0.40,
            edge_fade_height: px(30.),
            ui_font: "IBM Plex Mono".into(),
            text_font: "IBM Plex Mono".into(),
            font_size_body: px(15.),
            font_size_heading: [px(21.), px(18.), px(16.)],
            font_size_hint: px(10.),
            line_height_body: px(27.),
            font_size_overflow: px(11.),
            line_height_overflow: px(18.),
            column_width: px(585.),
            column_top: px(170.),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(&Settings::default())
    }
}

impl Global for Theme {}
