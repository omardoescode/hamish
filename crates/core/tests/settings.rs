use std::fs;
use std::path::{Path, PathBuf};

use ka_teb_core::{Setting, Settings, ThemeName, settings_path};

#[test]
fn the_settings_file_is_in_xdg_config_home() {
    assert_eq!(
        settings_path(Some("/cfg".as_ref()), Some(Path::new("/home/w"))),
        Some(PathBuf::from("/cfg/ka-teb/settings.json"))
    );
}

#[test]
fn without_xdg_config_home_the_settings_file_is_in_dot_config() {
    assert_eq!(
        settings_path(None, Some(Path::new("/home/w"))),
        Some(PathBuf::from("/home/w/.config/ka-teb/settings.json"))
    );
}

#[test]
fn a_relative_or_empty_xdg_config_home_is_ignored() {
    let home = Some(Path::new("/home/w"));
    let expected = Some(PathBuf::from("/home/w/.config/ka-teb/settings.json"));
    assert_eq!(settings_path(Some("cfg".as_ref()), home), expected);
    assert_eq!(settings_path(Some("".as_ref()), home), expected);
    assert_eq!(settings_path(None, None), None);
}

#[test]
fn defaults_are_the_dark_theme_and_the_bundled_font_at_spec_sizes() {
    let settings = Settings::default();
    assert_eq!(settings.theme, ThemeName::Dark);
    assert!((settings.ui_scale - 1.0).abs() < 1e-6);
    assert_eq!(settings.font_family, "IBM Plex Mono");
    assert!((settings.font_size - 15.0).abs() < 1e-6);
    assert!((settings.line_height - 1.8).abs() < 1e-6);
    assert!((settings.column_width - 585.0).abs() < 1e-6);
    assert!(settings.cycle_sounds);
}

#[test]
fn saved_settings_open_again_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ka-teb").join("settings.json");
    let settings = Settings {
        theme: ThemeName::Light,
        ui_scale: 1.2,
        font_family: "DejaVu Serif".to_owned(),
        font_size: 18.0,
        line_height: 1.5,
        column_width: 700.0,
        cycle_sounds: false,
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
fn a_field_not_in_the_file_takes_its_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "theme": "light", "font_size": 20 }"#).unwrap();

    let loaded = Settings::load(&path);

    assert_eq!(
        loaded.settings,
        Settings {
            theme: ThemeName::Light,
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
        r#"{ "font_size": 300, "line_height": 2.0, "font_family": " " }"#,
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
        problem.ends_with("out of limits, default used: font_size, font_family"),
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
fn ten_steps_of_a_tenth_land_on_the_tenth() {
    let mut settings = Settings::default();
    for _ in 0..10 {
        settings.increase(Setting::LineHeight);
    }
    assert_eq!(settings.line_height, 2.8);
}
