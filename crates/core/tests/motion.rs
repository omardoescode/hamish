use std::time::Duration;

use hamish_core::{Effect, caret_shown, next_caret_toggle};

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn the_variant_fade_in_runs_150_ms() {
    let fade = Effect::VariantFadeIn;
    assert_eq!(fade.duration(), ms(150));
    assert!(fade.progress(ms(0), false).abs() < 1e-6);
    assert!((fade.progress(ms(75), false) - 0.5).abs() < 1e-6);
    assert!((fade.progress(ms(150), false) - 1.0).abs() < 1e-6);
    assert!((fade.progress(ms(400), false) - 1.0).abs() < 1e-6);
}

#[test]
fn the_explosion_runs_650_ms() {
    let explosion = Effect::Explosion;
    assert_eq!(explosion.duration(), ms(650));
    assert!((explosion.progress(ms(325), false) - 0.5).abs() < 1e-6);
}

#[test]
fn the_undo_burst_runs_450_ms() {
    let burst = Effect::UndoBurst;
    assert_eq!(burst.duration(), ms(450));
    assert!((burst.progress(ms(225), false) - 0.5).abs() < 1e-6);
}

#[test]
fn the_focus_zoom_runs_150_ms() {
    let zoom = Effect::FocusZoom;
    assert_eq!(zoom.duration(), ms(150));
    assert!((zoom.progress(ms(75), false) - 0.5).abs() < 1e-6);
    assert!((zoom.progress(ms(0), true) - 1.0).abs() < 1e-6);
}

#[test]
fn with_reduced_motion_an_effect_is_done_at_once() {
    assert!((Effect::VariantFadeIn.progress(ms(0), true) - 1.0).abs() < 1e-6);
    assert!((Effect::Explosion.progress(ms(0), true) - 1.0).abs() < 1e-6);
    assert!((Effect::UndoBurst.progress(ms(0), true) - 1.0).abs() < 1e-6);
}

#[test]
fn a_caret_blink_round_is_on_for_500_ms_then_off_for_500_ms() {
    assert_eq!(Effect::CaretBlink.duration(), ms(1000));
    assert!(caret_shown(ms(0), false));
    assert!(caret_shown(ms(499), false));
    assert!(!caret_shown(ms(500), false));
    assert!(!caret_shown(ms(999), false));
    assert!(caret_shown(ms(1200), false));
}

#[test]
fn with_reduced_motion_the_caret_stays_on() {
    assert!(caret_shown(ms(700), true));
}

#[test]
fn a_blinking_caret_turns_on_or_off_at_each_500_ms() {
    assert_eq!(next_caret_toggle(ms(0)), ms(500));
    assert_eq!(next_caret_toggle(ms(120)), ms(380));
    assert_eq!(next_caret_toggle(ms(500)), ms(500));
    assert_eq!(next_caret_toggle(ms(1730)), ms(270));
}
