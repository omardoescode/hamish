//! Named themes: the palette of each one, as plain `0xRRGGBB` colors on the Theme color
//! roles. The app turns a palette into GPUI colors.

use serde::{Deserialize, Serialize};

/// A named theme. Ka-teb is the default. A settings file with the names `dark` or `light`
/// loads as Ka-teb or Ka-teb Light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeName {
    #[default]
    #[serde(alias = "dark")]
    KaTeb,
    #[serde(alias = "light")]
    KaTebLight,
    RosePine,
    RosePineDawn,
    OneDark,
    OneLight,
    CatppuccinMocha,
    GruvboxDark,
    TokyoNight,
    Nord,
}

/// The colors of one theme, each `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Light text on a dark background.
    pub dark: bool,
    pub background: u32,
    pub panel_background: u32,
    pub panel_border: u32,
    pub elevated_background: u32,
    pub elevated_row_hover: u32,
    pub text: u32,
    /// The title and Markdown headings.
    pub text_title: u32,
    /// At least 4.5:1 contrast on `background` (WCAG AA body text).
    pub text_muted: u32,
    pub text_faint: u32,
    pub text_quote: u32,
    pub list_marker: u32,
    pub accent: u32,
    pub accent_outline: u32,
    /// Slot underlines and empty Slot dots.
    pub slot_underline: u32,
    pub slot_dot_filled: u32,
    pub slot_paragraph_line: u32,
    pub selection: u32,
    /// Lab mark color; the app draws it translucent.
    pub lab_mark: u32,
}

impl ThemeName {
    /// Each theme, in the order the Settings modal lists them.
    pub const ALL: [Self; 10] = [
        Self::KaTeb,
        Self::KaTebLight,
        Self::RosePine,
        Self::RosePineDawn,
        Self::OneDark,
        Self::OneLight,
        Self::CatppuccinMocha,
        Self::GruvboxDark,
        Self::TokyoNight,
        Self::Nord,
    ];

    /// The name the writer sees.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::KaTeb => "Ka-teb",
            Self::KaTebLight => "Ka-teb Light",
            Self::RosePine => "Rosé Pine",
            Self::RosePineDawn => "Rosé Pine Dawn",
            Self::OneDark => "One Dark",
            Self::OneLight => "One Light",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::GruvboxDark => "Gruvbox Dark",
            Self::TokyoNight => "Tokyo Night",
            Self::Nord => "Nord",
        }
    }

    /// The name in the settings file, in kebab case.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::KaTeb => "ka-teb",
            Self::KaTebLight => "ka-teb-light",
            Self::RosePine => "rose-pine",
            Self::RosePineDawn => "rose-pine-dawn",
            Self::OneDark => "one-dark",
            Self::OneLight => "one-light",
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::GruvboxDark => "gruvbox-dark",
            Self::TokyoNight => "tokyo-night",
            Self::Nord => "nord",
        }
    }

    /// The colors of this theme.
    #[must_use]
    pub const fn palette(self) -> Palette {
        match self {
            Self::KaTeb => KA_TEB,
            Self::KaTebLight => KA_TEB_LIGHT,
            Self::RosePine => ROSE_PINE,
            Self::RosePineDawn => ROSE_PINE_DAWN,
            Self::OneDark => ONE_DARK,
            Self::OneLight => ONE_LIGHT,
            Self::CatppuccinMocha => CATPPUCCIN_MOCHA,
            Self::GruvboxDark => GRUVBOX_DARK,
            Self::TokyoNight => TOKYO_NIGHT,
            Self::Nord => NORD,
        }
    }
}

const KA_TEB: Palette = Palette {
    dark: true,
    background: 0x0007_0a1e,
    panel_background: 0x000c_0f21,
    panel_border: 0x001a_1d30,
    elevated_background: 0x0012_1424,
    elevated_row_hover: 0x0027_2531,
    text: 0x00e6_d2c6,
    text_title: 0x00fb_e5df,
    text_muted: 0x007e_7982,
    text_faint: 0x001e_2437,
    text_quote: 0x00a3_96a0,
    list_marker: 0x008a_85cf,
    accent: 0x008a_85cf,
    accent_outline: 0x007f_7db3,
    slot_underline: 0x0065_6b97,
    slot_dot_filled: 0x008a_8eba,
    slot_paragraph_line: 0x0082_6359,
    selection: 0x0026_2a54,
    lab_mark: 0x008b_86cb,
};

const KA_TEB_LIGHT: Palette = Palette {
    dark: false,
    background: 0x00f7_f3ec,
    panel_background: 0x00ef_eae1,
    panel_border: 0x00de_d7ca,
    elevated_background: 0x00fd_fbf7,
    elevated_row_hover: 0x00ea_e4d8,
    text: 0x002b_2824,
    text_title: 0x0017_1512,
    text_muted: 0x0072_6d61,
    text_faint: 0x00c9_c2b4,
    text_quote: 0x00a3_96a0,
    list_marker: 0x008a_85cf,
    accent: 0x0056_50ad,
    accent_outline: 0x006c_68a6,
    slot_underline: 0x008f_94b8,
    slot_dot_filled: 0x004f_5590,
    slot_paragraph_line: 0x00b0_8a7e,
    selection: 0x00d8_d5f0,
    lab_mark: 0x004c_45a1,
};

const ROSE_PINE: Palette = Palette {
    dark: true,
    background: 0x0019_1724,
    panel_background: 0x001f_1d2e,
    panel_border: 0x0026_233a,
    elevated_background: 0x0026_233a,
    elevated_row_hover: 0x0040_3d52,
    text: 0x00e0_def4,
    text_title: 0x00e0_def4,
    text_muted: 0x0090_8caa,
    text_faint: 0x0040_3d52,
    text_quote: 0x0090_8caa,
    list_marker: 0x00c4_a7e7,
    accent: 0x00c4_a7e7,
    accent_outline: 0x0090_8caa,
    slot_underline: 0x0031_748f,
    slot_dot_filled: 0x009c_cfd8,
    slot_paragraph_line: 0x00eb_bcba,
    selection: 0x0040_3d52,
    lab_mark: 0x00f6_c177,
};

const ROSE_PINE_DAWN: Palette = Palette {
    dark: false,
    background: 0x00fa_f4ed,
    panel_background: 0x00ff_faf3,
    panel_border: 0x00df_dad9,
    elevated_background: 0x00f2_e9e1,
    elevated_row_hover: 0x00df_dad9,
    text: 0x0057_5279,
    text_title: 0x0057_5279,
    text_muted: 0x0070_6b8a,
    text_faint: 0x00df_dad9,
    text_quote: 0x0079_7593,
    list_marker: 0x0090_7aa9,
    accent: 0x0090_7aa9,
    accent_outline: 0x0079_7593,
    slot_underline: 0x0056_949f,
    slot_dot_filled: 0x0028_6983,
    slot_paragraph_line: 0x00d7_827e,
    selection: 0x00df_dad9,
    lab_mark: 0x00ea_9d34,
};

const ONE_DARK: Palette = Palette {
    dark: true,
    background: 0x0028_2c34,
    panel_background: 0x0021_252b,
    panel_border: 0x0018_1a1f,
    elevated_background: 0x002c_313a,
    elevated_row_hover: 0x003a_3f4b,
    text: 0x00ab_b2bf,
    text_title: 0x00d7_dae0,
    text_muted: 0x008e_95a2,
    text_faint: 0x003b_4048,
    text_quote: 0x0082_8997,
    list_marker: 0x00c6_78dd,
    accent: 0x0061_afef,
    accent_outline: 0x0052_8bff,
    slot_underline: 0x00c6_78dd,
    slot_dot_filled: 0x0061_afef,
    slot_paragraph_line: 0x00d1_9a66,
    selection: 0x003e_4451,
    lab_mark: 0x00e5_c07b,
};

const ONE_LIGHT: Palette = Palette {
    dark: false,
    background: 0x00fa_fafa,
    panel_background: 0x00ea_eaeb,
    panel_border: 0x00db_dbdc,
    elevated_background: 0x00ff_ffff,
    elevated_row_hover: 0x00e5_e5e6,
    text: 0x0038_3a42,
    text_title: 0x0023_2324,
    text_muted: 0x0069_6c77,
    text_faint: 0x00db_dbdc,
    text_quote: 0x0069_6c77,
    list_marker: 0x00a6_26a4,
    accent: 0x0040_78f2,
    accent_outline: 0x0052_6fff,
    slot_underline: 0x00a6_26a4,
    slot_dot_filled: 0x0040_78f2,
    slot_paragraph_line: 0x00c1_8401,
    selection: 0x00e5_e5e6,
    lab_mark: 0x00c1_8401,
};

const CATPPUCCIN_MOCHA: Palette = Palette {
    dark: true,
    background: 0x001e_1e2e,
    panel_background: 0x0018_1825,
    panel_border: 0x0031_3244,
    elevated_background: 0x0031_3244,
    elevated_row_hover: 0x0045_475a,
    text: 0x00cd_d6f4,
    text_title: 0x00cd_d6f4,
    text_muted: 0x0093_99b2,
    text_faint: 0x0045_475a,
    text_quote: 0x00a6_adc8,
    list_marker: 0x00cb_a6f7,
    accent: 0x00cb_a6f7,
    accent_outline: 0x00b4_befe,
    slot_underline: 0x0089_b4fa,
    slot_dot_filled: 0x00b4_befe,
    slot_paragraph_line: 0x00eb_a0ac,
    selection: 0x0045_475a,
    lab_mark: 0x00f9_e2af,
};

const GRUVBOX_DARK: Palette = Palette {
    dark: true,
    background: 0x0028_2828,
    panel_background: 0x001d_2021,
    panel_border: 0x003c_3836,
    elevated_background: 0x0032_302f,
    elevated_row_hover: 0x0050_4945,
    text: 0x00eb_dbb2,
    text_title: 0x00fb_f1c7,
    text_muted: 0x00a8_9984,
    text_faint: 0x0050_4945,
    text_quote: 0x00a8_9984,
    list_marker: 0x00fa_bd2f,
    accent: 0x00fa_bd2f,
    accent_outline: 0x00d7_9921,
    slot_underline: 0x0045_8588,
    slot_dot_filled: 0x0083_a598,
    slot_paragraph_line: 0x00d6_5d0e,
    selection: 0x0050_4945,
    lab_mark: 0x00fa_bd2f,
};

const TOKYO_NIGHT: Palette = Palette {
    dark: true,
    background: 0x001a_1b26,
    panel_background: 0x0016_161e,
    panel_border: 0x0029_2e42,
    elevated_background: 0x001f_2335,
    elevated_row_hover: 0x003b_4261,
    text: 0x00c0_caf5,
    text_title: 0x00c0_caf5,
    text_muted: 0x007a_83ae,
    text_faint: 0x003b_4261,
    text_quote: 0x00a9_b1d6,
    list_marker: 0x00bb_9af7,
    accent: 0x007a_a2f7,
    accent_outline: 0x003d_59a1,
    slot_underline: 0x003d_59a1,
    slot_dot_filled: 0x007a_a2f7,
    slot_paragraph_line: 0x00ff_9e64,
    selection: 0x0028_3457,
    lab_mark: 0x00e0_af68,
};

const NORD: Palette = Palette {
    dark: true,
    background: 0x002e_3440,
    panel_background: 0x003b_4252,
    panel_border: 0x0043_4c5e,
    elevated_background: 0x0043_4c5e,
    elevated_row_hover: 0x004c_566a,
    text: 0x00d8_dee9,
    text_title: 0x00ec_eff4,
    text_muted: 0x0094_9eb2,
    text_faint: 0x0043_4c5e,
    text_quote: 0x008f_bcbb,
    list_marker: 0x0088_c0d0,
    accent: 0x0088_c0d0,
    accent_outline: 0x005e_81ac,
    slot_underline: 0x005e_81ac,
    slot_dot_filled: 0x0088_c0d0,
    slot_paragraph_line: 0x00d0_8770,
    selection: 0x0043_4c5e,
    lab_mark: 0x00eb_cb8b,
};

/// The WCAG 2 contrast ratio of two `0xRRGGBB` colors, from 1 (same) to 21 (black on
/// white). The order of the two colors does not matter.
///
/// ```
/// let ratio = ka_teb_core::contrast_ratio(0x0077_7777, 0x00ff_ffff);
/// assert!((ratio - 4.48).abs() < 0.01);
/// ```
#[must_use]
pub fn contrast_ratio(a: u32, b: u32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luminance(color: u32) -> f32 {
    let [_, red, green, blue] = color.to_be_bytes();
    let linear = |channel: u8| {
        let value = f32::from(channel) / 255.;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
}

#[cfg(test)]
mod tests {
    use super::ThemeName;

    #[test]
    fn each_slug_is_the_name_in_the_settings_file() {
        for name in ThemeName::ALL {
            let json = serde_json::to_string(&name).unwrap();
            assert_eq!(json, format!("\"{}\"", name.slug()));
        }
    }
}
