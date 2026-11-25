use ka_teb_core::{Effect, Particle, ParticleArea};

/// A span of 80 x 27 px, one line high.
const SPAN: ParticleArea = ParticleArea {
    width: 80.,
    height: 27.,
    line_height: 27.,
};

fn inside(particle: &Particle, margin: f32) -> bool {
    (-margin..=SPAN.width + margin).contains(&particle.x)
        && (-margin..=SPAN.height + margin).contains(&particle.y)
}

#[test]
fn sparks_stay_within_about_one_line_around_the_span() {
    for effect in [Effect::Explosion, Effect::UndoBurst] {
        for step in 0..10_u8 {
            let progress = f32::from(step) / 10.;
            for spark in effect.sparks(SPAN, 42, progress) {
                assert!(inside(&spark, 30.), "{effect:?} at {progress}");
            }
        }
    }
}

#[test]
fn exploding_sparks_fly_out_and_burst_sparks_fly_in() {
    let outside = |effect: Effect, progress| {
        effect
            .sparks(SPAN, 42, progress)
            .iter()
            .filter(|spark| !inside(spark, 0.))
            .count()
    };
    assert!(outside(Effect::Explosion, 0.9) > outside(Effect::Explosion, 0.0));
    assert!(outside(Effect::UndoBurst, 0.9) < outside(Effect::UndoBurst, 0.0));
}

#[test]
fn the_same_seed_gives_the_same_particles() {
    assert_eq!(
        Effect::Explosion.sparks(SPAN, 7, 0.5),
        Effect::Explosion.sparks(SPAN, 7, 0.5)
    );
    assert_ne!(
        Effect::Explosion.flames(SPAN, 7, 0.5),
        Effect::Explosion.flames(SPAN, 8, 0.5)
    );
}

#[test]
fn the_explosion_covers_the_old_text_then_clears_it() {
    let look = |progress| Effect::Explosion.particle_look(progress).unwrap();
    assert!((look(0.).cover - 1.).abs() < 1e-6);
    assert!((look(0.45).cover - 1.).abs() < 1e-6);
    assert!(look(1.).cover.abs() < 1e-6);
    assert!(look(1.).glow.abs() < 1e-6);
}

#[test]
fn effects_with_no_particles_have_none() {
    let area = SPAN;
    assert_eq!(Effect::VariantFadeIn.particle_look(0.5), None);
    assert_eq!(Effect::FocusZoom.sparks(area, 1, 0.5), []);
    assert_eq!(Effect::QuickFillDance.flames(area, 1, 0.5), []);
}
