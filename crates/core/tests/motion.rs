use std::time::Duration;

use ka_teb_core::Effect;

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
fn with_reduced_motion_an_effect_is_done_at_once() {
    assert!((Effect::VariantFadeIn.progress(ms(0), true) - 1.0).abs() < 1e-6);
    assert!((Effect::Explosion.progress(ms(0), true) - 1.0).abs() < 1e-6);
}
