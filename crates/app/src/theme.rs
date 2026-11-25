use gpui::{Global, Hsla, Pixels, hsla, px, rgb};

/// Theme values from the spec "Theme Mapping" table (estimates).
#[derive(Debug, Clone, Copy)]
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
    pub accent: Hsla,
    pub accent_outline: Hsla,
    pub slot_underline: Hsla,
    pub slot_dot: Hsla,
    pub slot_dot_filled: Hsla,
    pub slot_paragraph_line: Hsla,
    /// Background + about 4% white.
    pub slot_hover_background: Hsla,
    pub selection: Hsla,
    /// Background of Lab marks (mark style inferred, FR-52).
    pub lab_mark: Hsla,
    pub ghost_opacity: f32,
    pub edge_fade_height: Pixels,
    pub font_family: &'static str,
    pub font_size_body: Pixels,
    pub font_size_hint: Pixels,
    pub line_height_body: Pixels,
    pub font_size_overflow: Pixels,
    pub line_height_overflow: Pixels,
    /// About 65 ch of the body font: IBM Plex Mono advances 0.6 em, so 65 x 0.6 x 15 px.
    pub column_width: Pixels,
    /// Space above the title when scrolled to the start.
    pub column_top: Pixels,
}

impl Default for Theme {
    fn default() -> Self {
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
            edge_fade_height: px(30.),
            font_family: "IBM Plex Mono",
            font_size_body: px(15.),
            font_size_hint: px(10.),
            line_height_body: px(27.),
            font_size_overflow: px(11.),
            line_height_overflow: px(18.),
            column_width: px(585.),
            column_top: px(170.),
        }
    }
}

impl Global for Theme {}
