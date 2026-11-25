//! Timing of Editor work in a gpui test window over a long Document: a painted frame,
//! typing, caret moves at the start, undo and redo, Slot cycles, and the Alternatives and Settings panels.
//! The test text system shapes no real glyphs, so the times leave out font shaping.
#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a bench prints its timings and stops on a broken fixture"
)]

#[path = "../../core/benches/fixture/mod.rs"]
mod fixture;

use std::time::Instant;

use app::Workspace;
use gpui::{TestAppContext, VisualTestContext};

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// Runs `step` `count` times and prints the mean time of one run.
fn time(
    name: &str,
    count: u32,
    cx: &mut VisualTestContext,
    mut step: impl FnMut(&mut VisualTestContext),
) {
    let started = Instant::now();
    for _ in 0..count {
        step(cx);
    }
    let each = started.elapsed() / count;
    println!("{name:<28} {:>12.3} us", each.as_secs_f64() * 1e6);
}

fn main() {
    let only = std::env::args().nth(1).filter(|arg| !arg.starts_with('-'));
    let wanted = |name: &str| only.as_deref().is_none_or(|only| name.contains(only));
    let built = fixture::fixture();
    println!(
        "fixture: {} words, {} Slots, {} Word Slots",
        built.document.counter().words,
        built.document.slots().len(),
        built.word_slots.len()
    );
    let mut app = TestAppContext::single();
    assert!(app.update(app::init).is_ok(), "bundled font loads");
    let (_workspace, cx) =
        app.add_window_view(|window, cx| Workspace::new(built.document, window, cx));
    cx.run_until_parked();

    if wanted("frame") {
        time("frame", 50, cx, frame);
    }
    if wanted("type_char") {
        time("type_char + frame", 50, cx, |cx| {
            cx.simulate_input("x");
            frame(cx);
        });
    }
    if wanted("caret_move") {
        cx.simulate_keystrokes("home");
        time("caret_move + frame", 50, cx, |cx| {
            cx.simulate_keystrokes("ctrl-right");
            frame(cx);
        });
    }
    if wanted("undo_redo") {
        for _ in 0..20 {
            cx.simulate_input("y");
            cx.simulate_keystrokes("left right");
        }
        time("undo_redo + 2 frames", 20, cx, |cx| {
            cx.simulate_keystrokes("ctrl-z");
            frame(cx);
            cx.simulate_keystrokes("ctrl-shift-z");
            frame(cx);
        });
    }
    if wanted("slot_cycle") {
        cx.simulate_keystrokes("home");
        time("slot_cycle + frame", 50, cx, |cx| {
            cx.simulate_keystrokes("up");
            frame(cx);
        });
    }
    if wanted("alternatives") {
        cx.simulate_keystrokes("home");
        time("alternatives open + close", 20, cx, |cx| {
            cx.simulate_keystrokes("ctrl-shift-a");
            frame(cx);
            cx.simulate_keystrokes("escape");
            frame(cx);
        });
    }
    if wanted("settings") {
        time("settings open + close", 20, cx, |cx| {
            cx.simulate_keystrokes("ctrl-,");
            frame(cx);
            cx.simulate_keystrokes("escape");
            frame(cx);
        });
    }
}
