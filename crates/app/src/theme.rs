use gpui::{Global, Hsla, Pixels, SharedString, hsla, px, rgb};
use ka_teb_core::{Settings, ThemeName};

/// Theme values. Dark is the spec "Theme Mapping" table (estimates); light uses the same
/// roles on paper colors. Sizes come from [`Settings`].
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
    /// Background + about 4% white (dark) or black (light).
    pub slot_hover_background: Hsla,
    pub selection: Hsla,
    /// Background of Lab marks (mark style inferred, FR-52).
    pub lab_mark: Hsla,
    pub ghost_opacity: f32,
    /// Text of the cuts of a Trim proposal.
    pub trim_fade_opacity: f32,
    /// Text outside the current paragraph while the Alternatives panel is open.
    pub panel_open_dim_opacity: f32,
    pub edge_fade_height: Pixels,
    pub font_family: SharedString,
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
    /// The theme for `settings`: its colors, body font, and the Chrome at its UI scale.
    #[must_use]
    pub fn new(settings: &Settings) -> Self {
        let scale = settings.ui_scale;
        let base = match settings.theme {
            ThemeName::Dark => Self::dark(),
            ThemeName::Light => Self::light(),
        };
        Self {
            panel_width: base.panel_width * scale,
            elevated_radius: base.elevated_radius * scale,
            font_size_hint: base.font_size_hint * scale,
            font_size_overflow: base.font_size_overflow * scale,
            line_height_overflow: base.line_height_overflow * scale,
            font_family: settings.font_family.clone().into(),
            font_size_body: px(settings.font_size),
            font_size_heading: base
                .font_size_heading
                .map(|size| size * (settings.font_size / 15.)),
            line_height_body: px(settings.font_size * settings.line_height),
            column_width: px(settings.column_width),
            ..base
        }
    }

    fn dark() -> Self {
        Self {
            background: rgb(0x0007_0a1e).into(),
            panel_background: rgb(0x000c_0f21).into(),
            panel_border: rgb(0x001a_1d30).into(),
            panel_width: px(205.),
            elevated_background: rgb(0x0012_1424).into(),
            elevated_row_hover: rgb(0x0027_2531).into(),
            elevated_radius: px(6.),
            text: rgb(0x00e6_d2c6).into(),
            text_title: rgb(0x00fb_e5df).into(),
            text_muted: rgb(0x005a_5a6a).into(),
            text_faint: rgb(0x001e_2437).into(),
            text_heading: rgb(0x00fb_e5df).into(),
            text_quote: rgb(0x00a3_96a0).into(),
            list_marker: rgb(0x008a_85cf).into(),
            code_background: hsla(0., 0., 1., 0.06),
            accent: rgb(0x008a_85cf).into(),
            accent_outline: rgb(0x007f_7db3).into(),
            slot_underline: rgb(0x0065_6b97).into(),
            slot_dot: rgb(0x0065_6b97).into(),
            slot_dot_filled: rgb(0x008a_8eba).into(),
            slot_paragraph_line: rgb(0x0082_6359).into(),
            slot_hover_background: hsla(0., 0., 1., 0.04),
            selection: rgb(0x0026_2a54).into(),
            lab_mark: hsla(0.68, 0.4, 0.66, 0.18),
            ghost_opacity: 0.10,
            trim_fade_opacity: 0.22,
            panel_open_dim_opacity: 0.40,
            edge_fade_height: px(30.),
            font_family: "IBM Plex Mono".into(),
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

    fn light() -> Self {
        Self {
            background: rgb(0x00f7_f3ec).into(),
            panel_background: rgb(0x00ef_eae1).into(),
            panel_border: rgb(0x00de_d7ca).into(),
            elevated_background: rgb(0x00fd_fbf7).into(),
            elevated_row_hover: rgb(0x00ea_e4d8).into(),
            text: rgb(0x002b_2824).into(),
            text_title: rgb(0x0017_1512).into(),
            text_heading: rgb(0x0017_1512).into(),
            text_muted: rgb(0x0086_8072).into(),
            text_faint: rgb(0x00c9_c2b4).into(),
            accent: rgb(0x0056_50ad).into(),
            accent_outline: rgb(0x006c_68a6).into(),
            slot_underline: rgb(0x008f_94b8).into(),
            slot_dot: rgb(0x008f_94b8).into(),
            slot_dot_filled: rgb(0x004f_5590).into(),
            slot_paragraph_line: rgb(0x00b0_8a7e).into(),
            slot_hover_background: hsla(0., 0., 0., 0.04),
            selection: rgb(0x00d8_d5f0).into(),
            lab_mark: hsla(0.68, 0.4, 0.45, 0.16),
            ..Self::dark()
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(&Settings::default())
    }
}

impl Global for Theme {}
