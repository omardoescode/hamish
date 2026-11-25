use std::time::{Duration, Instant};

use hamish_core::{FocusZoom, paragraph_index, paragraph_range};

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn near(value: f32, expected: f32) -> bool {
    (value - expected).abs() < 1e-4
}

#[test]
fn a_focused_paragraph_eases_from_1_to_1_02_over_150_ms() {
    let start = Instant::now();
    let mut zoom = FocusZoom::default();

    zoom.focus(Some(1), start, false);

    assert!(near(zoom.scale(1, start, false), 1.0));
    assert!(zoom.scale(1, start + ms(30), false) < 1.004, "eases in");
    assert!(near(zoom.scale(1, start + ms(75), false), 1.01));
    assert!(near(zoom.scale(1, start + ms(150), false), 1.02));
    assert!(near(zoom.scale(1, start + ms(900), false), 1.02));
    assert!(near(zoom.scale(0, start + ms(150), false), 1.0));
}

#[test]
fn when_the_caret_moves_on_the_old_paragraph_zooms_out_as_the_new_one_zooms_in() {
    let start = Instant::now();
    let mut zoom = FocusZoom::default();
    zoom.focus(Some(0), start, false);
    let moved = start + ms(500);

    zoom.focus(Some(2), moved, false);

    assert!(near(zoom.scale(0, moved + ms(75), false), 1.01));
    assert!(near(zoom.scale(2, moved + ms(75), false), 1.01));
    assert!(near(zoom.scale(0, moved + ms(150), false), 1.0));
    assert!(near(zoom.scale(2, moved + ms(150), false), 1.02));
}

#[test]
fn a_paragraph_focused_again_mid_zoom_out_zooms_in_from_where_it_is() {
    let start = Instant::now();
    let mut zoom = FocusZoom::default();
    zoom.focus(Some(0), start, false);
    zoom.focus(Some(1), start + ms(500), false);

    zoom.focus(Some(0), start + ms(575), false);

    assert!(near(zoom.scale(0, start + ms(575), false), 1.01));
    assert!(near(zoom.scale(0, start + ms(725), false), 1.02));
}

#[test]
fn no_focus_zooms_all_out() {
    let start = Instant::now();
    let mut zoom = FocusZoom::default();
    zoom.focus(Some(3), start, false);

    zoom.focus(None, start + ms(500), false);

    assert!(near(zoom.scale(3, start + ms(575), false), 1.01));
    assert!(near(zoom.scale(3, start + ms(650), false), 1.0));
}

#[test]
fn with_reduced_motion_the_zoom_is_at_once() {
    let start = Instant::now();
    let mut zoom = FocusZoom::default();
    zoom.focus(Some(0), start, true);
    assert!(near(zoom.scale(0, start, true), 1.02));

    zoom.focus(Some(1), start, true);
    assert!(near(zoom.scale(0, start, true), 1.0));
    assert!(near(zoom.scale(1, start, true), 1.02));
}

#[test]
fn zoomed_lists_each_paragraph_not_at_its_normal_size() {
    let start = Instant::now();
    let mut zoom = FocusZoom::default();
    zoom.focus(Some(0), start, false);
    zoom.focus(Some(1), start + ms(500), false);

    let zoomed = zoom.zoomed(start + ms(575), false);
    assert_eq!(zoomed.len(), 2);
    assert!(zoom.is_moving(start + ms(575), false));

    let zoomed = zoom.zoomed(start + ms(650), false);
    assert_eq!(zoomed.len(), 1);
    assert_eq!(zoomed[0].0, 1);
    assert!(near(zoomed[0].1, 1.02));
    assert!(!zoom.is_moving(start + ms(650), false));
}

#[test]
fn a_paragraph_is_a_line_of_the_text() {
    let text = "Title\nOne two.\n\nThree.";
    assert_eq!(paragraph_index(text, 0), 0);
    assert_eq!(paragraph_index(text, 6), 1);
    assert_eq!(paragraph_index(text, 14), 1);
    assert_eq!(paragraph_index(text, 15), 2);
    assert_eq!(paragraph_index(text, 17), 3);
    assert_eq!(paragraph_range(text, 1), Some(6..14));
    assert_eq!(paragraph_range(text, 2), Some(15..15));
    assert_eq!(paragraph_range(text, 3), Some(16..22));
    assert_eq!(paragraph_range(text, 4), None);
}
