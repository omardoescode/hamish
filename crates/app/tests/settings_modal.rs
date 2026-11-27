mod system_text;

use std::fs;
use std::path::PathBuf;

use app::{Notice, Theme, Workspace};
use gpui::{Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext, point, px, rgb};
use hamish_core::{Document, Settings, ThemeName, Toggle};

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

#[test]
fn ctrl_comma_opens_settings_and_escape_closes_them() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx);
        assert!(!settings_open(&workspace, cx));

        cx.simulate_keystrokes("ctrl-,");
        assert!(settings_open(&workspace, cx));

        cx.simulate_keystrokes("escape");
        assert!(!settings_open(&workspace, cx));
    });
}

#[test]
fn after_settings_close_typing_goes_to_the_document() {
    system_text::run(1, |cx| {
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
    });
}

#[test]
fn the_settings_icon_in_the_chrome_opens_settings() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx);
        cx.simulate_click(point(px(30.), px(28.)), Modifiers::none());

        click(cx, "settings-button");

        assert!(settings_open(&workspace, cx));
    });
}

#[test]
fn hamish_light_from_the_theme_select_changes_the_editor_at_once() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);
        assert_eq!(theme(cx).background, rgb(0x0007_0a1e).into());

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-theme-select");
        click(cx, "settings-theme-hamish-light");

        assert_eq!(theme(cx).background, rgb(0x00f7_f3ec).into());
        assert_eq!(theme(cx).text, rgb(0x002b_2824).into());
    });
}

#[test]
fn a_named_theme_from_the_theme_select_applies_and_saves() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (_, cx) = open_with_file(cx, path.clone());

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-theme-select");
        click(cx, "settings-theme-rose-pine");

        assert_eq!(theme(cx).background, rgb(0x0019_1724).into());
        assert_eq!(theme(cx).accent, rgb(0x00c4_a7e7).into());
        assert_eq!(Settings::load(&path).settings.theme, ThemeName::RosePine);
    });
}

#[test]
fn a_larger_ui_scale_grows_the_chrome_text() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-ui-scale-up");

        let hint = theme(cx).font_size_hint;
        assert!((hint - px(11.)).abs() < px(0.01), "{hint:?}");
    });
}

#[test]
fn a_larger_font_size_grows_the_body_text_and_its_lines() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-section-editor");
        click(cx, "settings-font-size-up");

        let theme = theme(cx);
        assert_eq!(theme.font_size_body, px(16.));
        assert!((theme.line_height_body - px(28.8)).abs() < px(0.01));
    });
}

#[test]
fn line_height_and_column_width_change_the_editor() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-section-editor");
        click(cx, "settings-line-height-down");
        click(cx, "settings-column-width-up");

        let theme = theme(cx);
        assert!((theme.line_height_body - px(25.5)).abs() < px(0.01));
        assert_eq!(theme.column_width, px(605.));
    });
}

#[test]
fn a_font_from_the_ui_font_select_changes_the_ui_font() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (_, cx) = open_with_file(cx, path.clone());

        cx.simulate_keystrokes("ctrl-,");
        assert_eq!(theme(cx).ui_font, "IBM Plex Mono");

        click(cx, "settings-ui-font-select");
        click(cx, "settings-ui-font-1");
        assert_ne!(theme(cx).ui_font, "IBM Plex Mono");
        assert_ne!(Settings::load(&path).settings.ui_font, "IBM Plex Mono");

        click(cx, "settings-ui-font-select");
        click(cx, "settings-ui-font-0");
        assert_eq!(theme(cx).ui_font, "IBM Plex Mono");
        assert_eq!(Settings::load(&path).settings.ui_font, "IBM Plex Mono");
    });
}

#[test]
fn the_text_font_select_starts_at_default_and_changes_the_text_font() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (_, cx) = open_with_file(cx, path.clone());

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-section-editor");
        click(cx, "settings-text-font-select");
        click(cx, "settings-text-font-1");
        let chosen = Settings::load(&path).settings.text_font;
        assert!(
            chosen
                .as_deref()
                .is_some_and(|family| family != "IBM Plex Mono")
        );
        assert_eq!(Some(theme(cx).text_font.to_string()), chosen);
        assert_eq!(theme(cx).ui_font, "IBM Plex Mono");

        click(cx, "settings-text-font-select");
        click(cx, "settings-text-font-default");
        assert_eq!(Settings::load(&path).settings.text_font, None);
        assert_eq!(theme(cx).text_font, "IBM Plex Mono");
    });
}

#[test]
fn the_rows_of_a_font_select_have_one_height() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-section-editor");
        click(cx, "settings-text-font-select");

        let heights: Vec<_> = [
            "settings-text-font-default",
            "settings-text-font-0",
            "settings-text-font-1",
        ]
        .into_iter()
        .filter_map(|id| cx.debug_bounds(id))
        .map(|bounds| bounds.size.height)
        .collect();
        assert_eq!(heights.len(), 3);
        assert!(
            heights.iter().all(|height| *height == heights[0]),
            "{heights:?}"
        );
    });
}

/// The bounds of `id` in a fresh frame.
#[expect(clippy::expect_used, reason = "test fixture")]
fn bounds(cx: &mut VisualTestContext, id: &'static str) -> Bounds<Pixels> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds(id).expect("the window shows the element")
}

#[test]
fn typing_in_an_open_select_keeps_the_rows_that_contain_the_query() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-theme-select");
        cx.simulate_input("PINE");

        let rows = bounds(cx, "settings-select-rows");
        let rose_pine = bounds(cx, "settings-theme-rose-pine");
        let rose_pine_dawn = bounds(cx, "settings-theme-rose-pine-dawn");
        assert_eq!(rose_pine.origin.y, rows.origin.y);
        assert_eq!(
            rose_pine_dawn.origin.y,
            rows.origin.y + rose_pine.size.height
        );
        assert_eq!(rows.size.height, rose_pine.size.height * 2.);

        cx.simulate_keystrokes("backspace backspace backspace backspace");

        assert_eq!(bounds(cx, "settings-theme-hamish").origin.y, rows.origin.y);
    });
}

#[test]
fn enter_picks_the_first_row_the_search_keeps() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-theme-select");
        cx.simulate_input("dark");
        cx.simulate_keystrokes("enter");

        assert_eq!(theme(cx).background, rgb(0x0028_2c34).into());
    });
}

#[test]
fn down_and_up_move_the_row_that_enter_picks() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-theme-select");
        cx.simulate_input("dark");
        cx.simulate_keystrokes("down down up down enter");

        assert_eq!(theme(cx).background, rgb(0x0028_2828).into());
    });
}

#[test]
fn escape_closes_the_open_select_and_keeps_settings_open() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-theme-select");
        cx.simulate_keystrokes("escape");

        assert!(settings_open(&workspace, cx));

        cx.simulate_keystrokes("escape");

        assert!(!settings_open(&workspace, cx));
    });
}

#[test]
fn the_text_font_search_keeps_default_first_when_it_matches() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (_, cx) = open_with_file(cx, path.clone());

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-section-editor");
        click(cx, "settings-text-font-select");
        cx.simulate_input("plex mono");
        cx.simulate_keystrokes("enter");

        assert_eq!(
            Settings::load(&path).settings.text_font.as_deref(),
            Some("IBM Plex Mono")
        );

        click(cx, "settings-text-font-select");
        cx.simulate_input("DEF");
        let rows = bounds(cx, "settings-select-rows");
        assert_eq!(
            bounds(cx, "settings-text-font-default").origin.y,
            rows.origin.y
        );
        cx.simulate_keystrokes("enter");

        assert_eq!(Settings::load(&path).settings.text_font, None);
    });
}

#[test]
fn a_search_that_keeps_no_row_shows_no_match() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-ui-font-select");
        cx.simulate_input("qqqqqq");

        let no_match = bounds(cx, "settings-select-no-match");
        let search = bounds(cx, "settings-select-search");
        assert!(no_match.origin.y > search.origin.y);
        cx.simulate_keystrokes("enter");

        assert_eq!(theme(cx).ui_font, "IBM Plex Mono");
    });
}

#[test]
fn a_change_saves_to_the_settings_file() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hamish").join("settings.json");
        let (_, cx) = open_with_file(cx, path.clone());

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-theme-select");
        click(cx, "settings-theme-hamish-light");
        click(cx, "settings-section-editor");
        click(cx, "settings-font-size-up");

        let loaded = Settings::load(&path);
        assert!(loaded.problem.is_none());
        assert_eq!(
            loaded.settings,
            Settings {
                theme: ThemeName::HamishLight,
                font_size: 16.0,
                ..Settings::default()
            }
        );
    });
}

#[test]
fn saved_settings_apply_at_start() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{ "theme": "light", "column_width": 700 }"#).unwrap();

        let (workspace, cx) = open_with_file(cx, path);

        assert_eq!(theme(cx).background, rgb(0x00f7_f3ec).into());
        assert_eq!(theme(cx).column_width, px(700.));
        assert!(cx.read(|cx| workspace.read(cx).notice().is_none()));
    });
}

#[test]
fn a_bad_settings_file_gives_the_defaults_and_a_notice() {
    system_text::run(1, |cx| {
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
    });
}
#[test]
fn reduced_motion_turns_on_in_appearance_and_saves() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (_, cx) = open_with_file(cx, path.clone());

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-reduced-motion");

        assert!(Settings::load(&path).settings.is_on(Toggle::ReducedMotion));

        click(cx, "settings-reduced-motion");

        assert!(!Settings::load(&path).settings.is_on(Toggle::ReducedMotion));
    });
}

#[test]
fn focus_zoom_turns_off_in_appearance_and_saves() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (_, cx) = open_with_file(cx, path.clone());

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-focus-zoom");

        assert!(!Settings::load(&path).settings.is_on(Toggle::FocusZoom));

        click(cx, "settings-focus-zoom");

        assert!(Settings::load(&path).settings.is_on(Toggle::FocusZoom));
    });
}

#[test]
fn an_open_select_shows_a_search_icon_then_a_caret_in_its_search_field() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-ui-font-select");

        let search = bounds(cx, "settings-select-search");
        let icon = bounds(cx, "settings-select-search-icon");
        let caret = bounds(cx, "settings-select-caret");
        let placeholder = bounds(cx, "settings-select-placeholder");
        assert!(icon.left() >= search.left() && icon.right() <= caret.left());
        assert!(caret.right() <= placeholder.left());
        assert!(caret.top() >= search.top() && caret.bottom() <= search.bottom());
    });
}

#[test]
fn the_search_caret_moves_to_the_end_of_the_query() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-theme-select");
        let empty = bounds(cx, "settings-select-caret");
        cx.simulate_input("ros");
        let caret = bounds(cx, "settings-select-caret");
        let query = bounds(cx, "settings-select-query");

        assert!(caret.left() > empty.left());
        assert!(caret.left() >= query.right());
        assert!(cx.debug_bounds("settings-select-placeholder").is_none());
    });
}

#[test]
fn with_reduced_motion_the_search_caret_is_still() {
    system_text::run(1, |cx| {
        let (_, cx) = open(cx);

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-reduced-motion");
        click(cx, "settings-theme-select");

        bounds(cx, "settings-select-caret-still");
        assert!(cx.debug_bounds("settings-select-caret").is_none());
    });
}
