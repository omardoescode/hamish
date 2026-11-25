use app::{OpenAlternatives, Theme, ToggleChrome, ToggleOverflow, Workspace};
use gpui::{Bounds, Modifiers, Pixels, TestAppContext, VisualTestContext, px};
use ka_teb_core::{Document, LoadedSettings, Settings, ThemeName};

/// Opens a window with `settings` as the Theme and shows the Chrome.
fn open(cx: &mut TestAppContext, settings: Settings) -> &mut VisualTestContext {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    cx.update(|cx| {
        app::use_settings(
            None,
            LoadedSettings {
                settings,
                problem: None,
            },
            cx,
        );
    });
    let (_, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new("Title\nBody."), window, cx));
    cx.dispatch_action(ToggleChrome);
    cx.run_until_parked();
    cx
}

fn scaled(ui_scale: f32) -> Settings {
    Settings {
        ui_scale,
        ..Settings::default()
    }
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .expect("the window shows the element")
}

fn assert_above_bar(cx: &mut VisualTestContext, selector: &'static str) {
    let element = bounds(cx, selector);
    let bar = bounds(cx, "chrome-bar");
    assert!(
        element.bottom() <= bar.top(),
        "{selector} {element:?} overlaps the Chrome bar {bar:?}"
    );
}

#[gpui::test]
fn a_click_on_the_counter_toggles_the_chrome_and_keeps_the_caret(cx: &mut TestAppContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new("Title\nBody."), window, cx));
    cx.simulate_keystrokes("down end");
    let selection =
        |cx: &mut VisualTestContext| cx.read(|cx| workspace.read(cx).editor().read(cx).selection());
    assert_eq!(selection(cx), 11..11);

    let counter = bounds(cx, "counter");
    cx.simulate_click(counter.center(), Modifiers::none());

    assert!(cx.read(|cx| workspace.read(cx).chrome_visible()));
    assert_eq!(selection(cx), 11..11);
}

#[test]
fn the_chrome_bar_background_is_opaque_in_every_theme() {
    for name in ThemeName::ALL {
        let theme = Theme::new(&Settings {
            theme: name,
            ..Settings::default()
        });
        assert!(
            (theme.elevated_background.a - 1.).abs() < f32::EPSILON,
            "{name:?}"
        );
        assert_ne!(theme.elevated_background, theme.background, "{name:?}");
        assert_ne!(
            theme.elevated_background, theme.panel_background,
            "{name:?}"
        );
    }
}

#[gpui::test]
fn the_chrome_bar_holds_all_chrome_buttons(cx: &mut TestAppContext) {
    let cx = open(cx, Settings::default());

    let bar = bounds(cx, "chrome-bar");
    assert_eq!(bar.left(), px(0.), "the bar spans the window");
    for selector in [
        "new",
        "save",
        "settings-button",
        "overflow-button",
        "lab-button",
    ] {
        let button = bounds(cx, selector);
        assert!(
            bar.top() <= button.top() && button.bottom() <= bar.bottom(),
            "{selector} {button:?} is outside the Chrome bar {bar:?}"
        );
    }
}

#[gpui::test]
fn the_overflow_hint_ends_above_the_chrome_bar(cx: &mut TestAppContext) {
    let cx = open(cx, Settings::default());
    cx.dispatch_action(ToggleOverflow);
    cx.run_until_parked();

    assert_above_bar(cx, "overflow-hint");
}

#[gpui::test]
fn the_overflow_hint_ends_above_the_chrome_bar_at_a_large_ui_scale(cx: &mut TestAppContext) {
    let cx = open(cx, scaled(2.));
    cx.dispatch_action(ToggleOverflow);
    cx.run_until_parked();

    assert_above_bar(cx, "overflow-hint");
}

#[gpui::test]
fn the_alternatives_hint_ends_above_the_chrome_bar(cx: &mut TestAppContext) {
    let cx = open(cx, scaled(2.));
    cx.dispatch_action(OpenAlternatives);
    cx.run_until_parked();

    assert_above_bar(cx, "alternatives-hint");
}
