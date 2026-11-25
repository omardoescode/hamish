use std::fs;
use std::path::PathBuf;

use app::{Notice, Theme, Workspace};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point, px, rgb};
use ka_teb_core::{Document, Settings, ThemeName};

fn open(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new("Title\nBody."), window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

/// Opens the app with the settings file `path`, as the binary does at start.
fn open_with_file(
    cx: &mut TestAppContext,
    path: PathBuf,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let loaded = Settings::load(&path);
    cx.update(|cx| app::use_settings(Some(path), loaded, cx));
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new("Title\nBody."), window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn settings_open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).settings_open())
}

fn theme(cx: &mut VisualTestContext) -> Theme {
    cx.read(|cx| cx.global::<Theme>().clone())
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).expect("the window shows the element");
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn ctrl_comma_opens_settings_and_escape_closes_them(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx);
    assert!(!settings_open(&workspace, cx));

    cx.simulate_keystrokes("ctrl-,");
    assert!(settings_open(&workspace, cx));

    cx.simulate_keystrokes("escape");
    assert!(!settings_open(&workspace, cx));
}

#[gpui::test]
fn after_settings_close_typing_goes_to_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx);

    cx.simulate_keystrokes("ctrl-, escape");
    cx.simulate_input("A ");

    let text = cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .text()
            .to_owned()
    });
    assert_eq!(text, "A Title\nBody.");
}

#[gpui::test]
fn the_settings_icon_in_the_chrome_opens_settings(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx);
    cx.simulate_click(point(px(30.), px(28.)), Modifiers::none());

    click(cx, "settings-button");

    assert!(settings_open(&workspace, cx));
}

#[gpui::test]
fn ka_teb_light_from_the_theme_select_changes_the_editor_at_once(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    assert_eq!(theme(cx).background, rgb(0x0007_0a1e).into());

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-theme-select");
    click(cx, "settings-theme-ka-teb-light");

    assert_eq!(theme(cx).background, rgb(0x00f7_f3ec).into());
    assert_eq!(theme(cx).text, rgb(0x002b_2824).into());
}

#[gpui::test]
fn a_named_theme_from_the_theme_select_applies_and_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let (_, cx) = open_with_file(cx, path.clone());

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-theme-select");
    click(cx, "settings-theme-rose-pine");

    assert_eq!(theme(cx).background, rgb(0x0019_1724).into());
    assert_eq!(theme(cx).accent, rgb(0x00c4_a7e7).into());
    assert_eq!(Settings::load(&path).settings.theme, ThemeName::RosePine);
}

#[gpui::test]
fn a_larger_ui_scale_grows_the_chrome_text(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-ui-scale-up");

    let hint = theme(cx).font_size_hint;
    assert!((hint - px(11.)).abs() < px(0.01), "{hint:?}");
}

#[gpui::test]
fn a_larger_font_size_grows_the_body_text_and_its_lines(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-section-editor");
    click(cx, "settings-font-size-up");

    let theme = theme(cx);
    assert_eq!(theme.font_size_body, px(16.));
    assert!((theme.line_height_body - px(28.8)).abs() < px(0.01));
}

#[gpui::test]
fn line_height_and_column_width_change_the_editor(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-section-editor");
    click(cx, "settings-line-height-down");
    click(cx, "settings-column-width-up");

    let theme = theme(cx);
    assert!((theme.line_height_body - px(25.5)).abs() < px(0.01));
    assert_eq!(theme.column_width, px(605.));
}

#[gpui::test]
fn a_font_from_the_font_select_changes_the_setting(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let (_, cx) = open_with_file(cx, path.clone());

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-section-editor");
    assert_eq!(theme(cx).font_family, "IBM Plex Mono");

    click(cx, "settings-font-select");
    click(cx, "settings-font-1");
    assert_ne!(theme(cx).font_family, "IBM Plex Mono");
    assert_ne!(Settings::load(&path).settings.font_family, "IBM Plex Mono");

    click(cx, "settings-font-select");
    click(cx, "settings-font-0");
    assert_eq!(theme(cx).font_family, "IBM Plex Mono");
    assert_eq!(Settings::load(&path).settings.font_family, "IBM Plex Mono");
}

#[gpui::test]
fn a_change_saves_to_the_settings_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ka-teb").join("settings.json");
    let (_, cx) = open_with_file(cx, path.clone());

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-theme-select");
    click(cx, "settings-theme-ka-teb-light");
    click(cx, "settings-section-editor");
    click(cx, "settings-font-size-up");

    let loaded = Settings::load(&path);
    assert!(loaded.problem.is_none());
    assert_eq!(
        loaded.settings,
        Settings {
            theme: ThemeName::KaTebLight,
            font_size: 16.0,
            ..Settings::default()
        }
    );
}

#[gpui::test]
fn saved_settings_apply_at_start(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{ "theme": "light", "column_width": 700 }"#).unwrap();

    let (workspace, cx) = open_with_file(cx, path);

    assert_eq!(theme(cx).background, rgb(0x00f7_f3ec).into());
    assert_eq!(theme(cx).column_width, px(700.));
    assert!(cx.read(|cx| workspace.read(cx).notice().is_none()));
}

#[gpui::test]
fn a_bad_settings_file_gives_the_defaults_and_a_notice(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "{ theme: light").unwrap();

    let (workspace, cx) = open_with_file(cx, path);

    assert_eq!(theme(cx).background, rgb(0x0007_0a1e).into());
    let notice = cx.read(|cx| workspace.read(cx).notice().cloned());
    assert!(
        matches!(notice, Some(Notice::SettingsNotLoaded(_))),
        "{notice:?}"
    );
}
#[gpui::test]
fn reduced_motion_turns_on_in_appearance_and_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let (_, cx) = open_with_file(cx, path.clone());

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-reduced-motion-on");

    assert!(Settings::load(&path).settings.reduced_motion);

    click(cx, "settings-reduced-motion-off");

    assert!(!Settings::load(&path).settings.reduced_motion);
}

#[gpui::test]
fn focus_zoom_turns_off_in_appearance_and_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let (_, cx) = open_with_file(cx, path.clone());

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-focus-zoom-off");

    assert!(!Settings::load(&path).settings.focus_zoom);

    click(cx, "settings-focus-zoom-on");

    assert!(Settings::load(&path).settings.focus_zoom);
}
