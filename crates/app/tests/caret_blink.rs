mod system_text;

use std::time::Duration;

use app::Workspace;
use gpui::{Entity, TestAppContext, VisualTestContext};
use hamish_core::{Document, LoadedSettings, OnOff, Settings};

fn open(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let loaded = LoadedSettings {
        settings,
        problem: None,
    };
    cx.update(|cx| app::use_settings(None, loaded, cx));
    let document = Document::new("Hello world");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.update(|window, _| window.activate_window());
    wait(cx, 0);
    (workspace, cx)
}

/// Lets `millis` pass and paints the next frame.
fn wait(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn caret_shows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .caret_bounds()
            .is_some()
    })
}

fn overflow_caret_shows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| {
        workspace
            .read(cx)
            .overflow_panel()
            .read(cx)
            .caret_bounds()
            .is_some()
    })
}

#[test]
fn the_caret_shows_500_ms_then_hides_500_ms() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());

        assert!(caret_shows(&workspace, cx));
        wait(cx, 450);
        assert!(caret_shows(&workspace, cx));
        wait(cx, 100);
        assert!(!caret_shows(&workspace, cx));
        wait(cx, 500);
        assert!(caret_shows(&workspace, cx));
    });
}

#[test]
fn a_caret_move_starts_the_blink_again() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());

        wait(cx, 600);
        assert!(!caret_shows(&workspace, cx));
        cx.simulate_keystrokes("right");
        wait(cx, 0);
        assert!(caret_shows(&workspace, cx));
        wait(cx, 450);
        assert!(caret_shows(&workspace, cx));
        wait(cx, 100);
        assert!(!caret_shows(&workspace, cx));
    });
}

#[test]
fn typing_starts_the_blink_again() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());

        wait(cx, 600);
        cx.simulate_input("A");
        wait(cx, 0);
        assert!(caret_shows(&workspace, cx));
    });
}

#[test]
fn with_reduced_motion_the_caret_stays_on() {
    system_text::run(1, |cx| {
        let settings = Settings {
            reduced_motion: OnOff::On,
            ..Settings::default()
        };
        let (workspace, cx) = open(cx, settings);

        wait(cx, 600);
        assert!(caret_shows(&workspace, cx));
    });
}

#[test]
fn the_normal_mode_block_caret_blinks() {
    system_text::run(1, |cx| {
        let settings = Settings {
            vim_mode: OnOff::On,
            ..Settings::default()
        };
        let (workspace, cx) = open(cx, settings);

        assert!(caret_shows(&workspace, cx));
        wait(cx, 600);
        assert!(!caret_shows(&workspace, cx));
    });
}

#[test]
fn while_the_window_has_no_focus_the_caret_stays_on() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());

        cx.deactivate_window();
        wait(cx, 600);
        assert!(caret_shows(&workspace, cx));
    });
}

#[test]
fn the_overflow_caret_blinks() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open(cx, Settings::default());

        cx.simulate_keystrokes("ctrl-shift-o");
        wait(cx, 0);
        assert!(overflow_caret_shows(&workspace, cx));
        wait(cx, 600);
        assert!(!overflow_caret_shows(&workspace, cx));
        cx.simulate_input("A");
        wait(cx, 0);
        assert!(overflow_caret_shows(&workspace, cx));
    });
}
