use std::fs;
use std::path::{Path, PathBuf};

use hamish_core::{
    Direction, Setting, Settings, ThemeName, Toggle, contrast_ratio, hamishrc_path, settings_path,
};

#[test]
fn the_settings_file_is_in_xdg_config_home() {
    assert_eq!(
        settings_path(Some("/cfg".as_ref()), Some(Path::new("/home/w"))),
        Some(PathBuf::from("/cfg/hamish/settings.json"))
    );
}

#[test]
fn without_xdg_config_home_the_settings_file_is_in_dot_config() {
    assert_eq!(
        settings_path(None, Some(Path::new("/home/w"))),
        Some(PathBuf::from("/home/w/.config/hamish/settings.json"))
    );
}

#[test]
fn a_relative_or_empty_xdg_config_home_is_ignored() {
    let home = Some(Path::new("/home/w"));
    let expected = Some(PathBuf::from("/home/w/.config/hamish/settings.json"));
    assert_eq!(settings_path(Some("cfg".as_ref()), home), expected);
    assert_eq!(settings_path(Some("".as_ref()), home), expected);
    assert_eq!(settings_path(None, None), None);
}

#[test]
fn the_hamishrc_is_next_to_the_settings_file() {
    assert_eq!(
        hamishrc_path(Some("/cfg".as_ref()), Some(Path::new("/home/w"))),
        Some(PathBuf::from("/cfg/hamish/hamishrc"))
    );
    assert_eq!(
        hamishrc_path(None, Some(Path::new("/home/w"))),
        Some(PathBuf::from("/home/w/.config/hamish/hamishrc"))
    );
    assert_eq!(hamishrc_path(None, None), None);
}

#[test]
fn defaults_are_the_hamish_theme_and_the_bundled_font_at_the_default_sizes() {
    let settings = Settings::default();
    assert_eq!(settings.theme, ThemeName::Hamish);
    assert!((settings.ui_scale - 1.0).abs() < 1e-6);
    assert_eq!(settings.ui_font, "IBM Plex Mono");
    assert_eq!(settings.text_font, None);
    assert!((settings.font_size - 15.0).abs() < 1e-6);
    assert!((settings.line_height - 1.8).abs() < 1e-6);
    assert!((settings.column_width - 585.0).abs() < 1e-6);
    assert!(settings.cycle_sounds);
}

#[test]
fn saved_settings_open_again_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hamish").join("settings.json");
    let settings = Settings {
        theme: ThemeName::HamishLight,
        ui_scale: 1.2,
        ui_font: "DejaVu Sans".to_owned(),
        text_font: Some("DejaVu Serif".to_owned()),
        font_size: 18.0,
        line_height: 1.5,
        column_width: 700.0,
        reduced_motion: true,
        cycle_sounds: false,
        focus_zoom: false,
        vim_mode: true,
        markers: false,
        markdown_styles: false,
    };

    settings.save(&path).unwrap();
    let loaded = Settings::load(&path);

    assert_eq!(loaded.settings, settings);
    assert!(loaded.problem.is_none());
}

#[test]
fn a_missing_file_gives_the_defaults_and_no_problem() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = Settings::load(&dir.path().join("settings.json"));
    assert_eq!(loaded.settings, Settings::default());
    assert!(loaded.problem.is_none());
}

#[test]
fn a_file_that_is_not_settings_json_gives_the_defaults_and_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "{ theme: dark").unwrap();

    let loaded = Settings::load(&path);

    assert_eq!(loaded.settings, Settings::default());
    let problem = loaded.problem.unwrap().to_string();
    assert!(
        problem.starts_with(&format!("{}: ", path.display())),
        "{problem}"
    );
}

#[test]
fn the_default_text_font_is_the_ui_font() {
    let settings = Settings {
        ui_font: "Adwaita Sans".to_owned(),
        ..Settings::default()
    };
    assert_eq!(settings.text_font_family(), "Adwaita Sans");

    let settings = Settings {
        text_font: Some("DejaVu Serif".to_owned()),
        ..settings
    };
    assert_eq!(settings.text_font_family(), "DejaVu Serif");
    assert_eq!(settings.ui_font, "Adwaita Sans");
}

#[test]
fn an_old_file_with_one_font_gives_that_font_as_the_text_font() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(
        &path,
        r#"{ "font_family": "DejaVu Serif", "font_size": 18 }"#,
    )
    .unwrap();

    let loaded = Settings::load(&path);

    assert_eq!(
        loaded.settings,
        Settings {
            text_font: Some("DejaVu Serif".to_owned()),
            font_size: 18.0,
            ..Settings::default()
        }
    );
    assert_eq!(loaded.settings.ui_font, "IBM Plex Mono");
    assert!(loaded.problem.is_none());
}

#[test]
fn a_field_not_in_the_file_takes_its_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "theme": "light", "font_size": 20 }"#).unwrap();

    let loaded = Settings::load(&path);

    assert_eq!(
        loaded.settings,
        Settings {
            theme: ThemeName::HamishLight,
            font_size: 20.0,
            ..Settings::default()
        }
    );
    assert!(loaded.problem.is_none());
}

#[test]
fn a_value_out_of_its_limits_takes_its_default_and_names_it_in_the_problem() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(
        &path,
        r#"{ "font_size": 300, "line_height": 2.0, "ui_font": " ", "text_font": "" }"#,
    )
    .unwrap();

    let loaded = Settings::load(&path);

    assert_eq!(
        loaded.settings,
        Settings {
            line_height: 2.0,
            ..Settings::default()
        }
    );
    let problem = loaded.problem.unwrap().to_string();
    assert!(
        problem.ends_with("out of limits, default used: font_size, ui_font, text_font"),
        "{problem}"
    );
}

#[test]
fn a_step_moves_a_value_by_its_step() {
    let mut settings = Settings::default();
    settings.increase(Setting::FontSize);
    settings.increase(Setting::LineHeight);
    settings.decrease(Setting::UiScale);
    settings.decrease(Setting::ColumnWidth);

    assert!((settings.font_size - 16.0).abs() < 1e-6);
    assert!((settings.line_height - 1.9).abs() < 1e-6);
    assert!((settings.ui_scale - 0.9).abs() < 1e-6);
    assert!((settings.column_width - 565.0).abs() < 1e-6);
}

#[test]
fn a_step_stops_at_the_limits() {
    let mut settings = Settings {
        font_size: 40.0,
        ui_scale: 0.5,
        ..Settings::default()
    };
    settings.increase(Setting::FontSize);
    settings.decrease(Setting::UiScale);

    assert!((settings.font_size - 40.0).abs() < 1e-6);
    assert!((settings.ui_scale - 0.5).abs() < 1e-6);
}

#[test]
fn a_step_forward_goes_up_and_a_step_back_goes_down() {
    let mut settings = Settings::default();
    settings.step(Setting::FontSize, Direction::Forward);
    settings.step(Setting::UiScale, Direction::Back);

    assert!((settings.font_size - 16.0).abs() < 1e-6);
    assert!((settings.ui_scale - 0.9).abs() < 1e-6);
}

#[test]
fn set_turns_a_toggle_on_or_off_whatever_it_was() {
    let mut settings = Settings::default();
    settings.set(Toggle::Markers, true);
    settings.set(Toggle::VimMode, true);
    settings.set(Toggle::FocusZoom, false);
    settings.set(Toggle::ReducedMotion, false);

    assert_eq!(
        settings,
        Settings {
            vim_mode: true,
            focus_zoom: false,
            ..Settings::default()
        }
    );
}

#[test]
fn a_file_with_the_markdown_key_loads_markdown_styles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "markdown": false }"#).unwrap();

    let loaded = Settings::load(&path);

    assert!(loaded.problem.is_none());
    assert!(!loaded.settings.markdown_styles);
}

#[test]
fn ten_steps_of_a_tenth_land_on_the_tenth() {
    let mut settings = Settings::default();
    for _ in 0..10 {
        settings.increase(Setting::LineHeight);
    }
    assert_eq!(settings.line_height, 2.8);
}
#[test]
fn each_setting_has_a_label_and_a_slug() {
    let named: Vec<(&str, &str)> = [
        Setting::UiScale,
        Setting::FontSize,
        Setting::LineHeight,
        Setting::ColumnWidth,
    ]
    .into_iter()
    .map(|setting| (setting.label(), setting.slug()))
    .collect();
    assert_eq!(
        named,
        [
            ("UI scale", "ui-scale"),
            ("Font size", "font-size"),
            ("Line height", "line-height"),
            ("Column width", "column-width"),
        ]
    );
}

#[test]
fn a_shown_value_carries_its_unit() {
    let settings = Settings {
        ui_scale: 1.2,
        font_size: 18.0,
        line_height: 1.5,
        column_width: 700.0,
        ..Settings::default()
    };
    assert_eq!(settings.shown(Setting::UiScale), "120%");
    assert_eq!(settings.shown(Setting::FontSize), "18 px");
    assert_eq!(settings.shown(Setting::LineHeight), "1.5");
    assert_eq!(settings.shown(Setting::ColumnWidth), "700 px");
}

#[test]
fn each_toggle_has_a_label_and_a_slug() {
    let named: Vec<(&str, &str)> = [
        Toggle::ReducedMotion,
        Toggle::FocusZoom,
        Toggle::CycleSounds,
        Toggle::VimMode,
        Toggle::Markers,
        Toggle::MarkdownStyles,
    ]
    .into_iter()
    .map(|toggle| (toggle.label(), toggle.slug()))
    .collect();
    assert_eq!(
        named,
        [
            ("Reduced motion", "reduced-motion"),
            ("Focus zoom", "focus-zoom"),
            ("Cycle sounds", "cycle-sounds"),
            ("Vim mode", "vim-mode"),
            ("Markers", "markers"),
            ("Markdown styles", "markdown"),
        ]
    );
}

#[test]
fn a_flip_turns_a_toggle_on_or_off_and_shows_it() {
    let mut settings = Settings::default();
    assert_eq!(settings.shown_toggle(Toggle::ReducedMotion), "Off");

    settings.flip(Toggle::ReducedMotion);
    settings.flip(Toggle::FocusZoom);
    settings.flip(Toggle::CycleSounds);

    assert!(settings.is_on(Toggle::ReducedMotion));
    assert_eq!(settings.shown_toggle(Toggle::ReducedMotion), "On");
    assert_eq!(
        settings,
        Settings {
            reduced_motion: true,
            focus_zoom: false,
            cycle_sounds: false,
            ..Settings::default()
        }
    );
}

#[test]
fn markers_and_markdown_styles_are_on_by_default_and_in_a_file_without_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "theme": "light" }"#).unwrap();

    let loaded = Settings::load(&path).settings;

    assert!(Settings::default().is_on(Toggle::Markers));
    assert!(Settings::default().is_on(Toggle::MarkdownStyles));
    assert!(loaded.is_on(Toggle::Markers));
    assert!(loaded.is_on(Toggle::MarkdownStyles));
}

#[test]
fn vim_mode_is_off_by_default_and_in_a_file_without_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "theme": "light" }"#).unwrap();

    assert!(!Settings::default().vim_mode);
    assert!(!Settings::load(&path).settings.vim_mode);
}

#[test]
fn reduced_motion_is_off_by_default_and_in_a_file_without_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "theme": "light" }"#).unwrap();

    assert!(!Settings::default().reduced_motion);
    assert!(!Settings::load(&path).settings.reduced_motion);
}

#[test]
fn focus_zoom_is_on_by_default_and_in_a_file_without_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "theme": "light" }"#).unwrap();

    assert!(Settings::default().focus_zoom);
    assert!(Settings::load(&path).settings.focus_zoom);
}

#[test]
fn the_old_dark_and_light_names_load_as_hamish_and_hamish_light() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");

    fs::write(&path, r#"{ "theme": "dark" }"#).unwrap();
    assert_eq!(Settings::load(&path).settings.theme, ThemeName::Hamish);

    fs::write(&path, r#"{ "theme": "light" }"#).unwrap();
    assert_eq!(Settings::load(&path).settings.theme, ThemeName::HamishLight);
}

#[test]
fn a_named_theme_saves_by_its_name_and_loads_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let settings = Settings {
        theme: ThemeName::RosePineDawn,
        ..Settings::default()
    };

    settings.save(&path).unwrap();

    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains(r#""theme": "rose-pine-dawn""#)
    );
    assert_eq!(
        Settings::load(&path).settings.theme,
        ThemeName::RosePineDawn
    );
}

#[test]
fn the_theme_list_names_each_theme_in_order() {
    let labels: Vec<&str> = ThemeName::ALL.iter().map(|name| name.label()).collect();
    assert_eq!(
        labels,
        [
            "Hamish",
            "Hamish Light",
            "Rosé Pine",
            "Rosé Pine Dawn",
            "One Dark",
            "One Light",
            "Catppuccin Mocha",
            "Gruvbox Dark",
            "Tokyo Night",
            "Nord",
        ]
    );
}

#[test]
fn a_theme_palette_uses_the_published_colors() {
    let rose_pine = ThemeName::RosePine.palette();
    assert_eq!(rose_pine.background, 0x0019_1724);
    assert_eq!(rose_pine.text, 0x00e0_def4);
    assert_eq!(rose_pine.accent, 0x00c4_a7e7);
    assert!(rose_pine.dark);

    let nord = ThemeName::Nord.palette();
    assert_eq!(nord.background, 0x002e_3440);
    assert_eq!(nord.accent, 0x0088_c0d0);

    assert!(!ThemeName::OneLight.palette().dark);
    assert_eq!(ThemeName::OneLight.palette().background, 0x00fa_fafa);
}

#[test]
fn contrast_ratio_follows_the_wcag_worked_values() {
    assert!((contrast_ratio(0x00ff_ffff, 0x0000_0000) - 21.0).abs() < 0.01);
    assert!((contrast_ratio(0x0000_0000, 0x00ff_ffff) - 21.0).abs() < 0.01);
    assert!((contrast_ratio(0x0077_7777, 0x00ff_ffff) - 4.48).abs() < 0.01);
    assert!((contrast_ratio(0x005a_5a6a, 0x0007_0a1e) - 2.90).abs() < 0.01);
}

#[test]
fn muted_text_reads_on_the_background_in_each_theme() {
    for name in ThemeName::ALL {
        let palette = name.palette();
        let ratio = contrast_ratio(palette.text_muted, palette.background);
        assert!(ratio >= 4.5, "{}: {ratio:.2}", name.label());
    }
}

#[test]
fn placeholder_text_is_halfway_from_muted_text_to_text() {
    assert_eq!(
        ThemeName::CatppuccinMocha.palette().text_placeholder(),
        0x00b0_b8d3
    );
}

#[test]
fn placeholder_text_reads_on_a_popover_in_each_theme() {
    for name in ThemeName::ALL {
        let palette = name.palette();
        let ratio = contrast_ratio(palette.text_placeholder(), palette.elevated_background);
        assert!(ratio >= 4.5, "{}: {ratio:.2}", name.label());
    }
}
