//! Timing of core Document work over a long Document: typing, caret moves, undo and redo,
//! Slot cycles, per-frame queries, save and open, and the Alternatives list.
#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a bench prints its timings and stops on a broken fixture"
)]

mod fixture;

use std::hint::black_box;
use std::time::{Duration, Instant};

use fixture::fixture;
use hamish_core::{AlternativesList, Caret, Cycle, Document, EditSession, Motion};

/// Runs `step` `count` times and prints the mean time of one run.
fn time(name: &str, count: u32, mut step: impl FnMut()) -> Duration {
    let started = Instant::now();
    for _ in 0..count {
        step();
    }
    let each = started.elapsed() / count;
    println!("{name:<28} {:>12.3} us", each.as_secs_f64() * 1e6);
    each
}

fn middle(document: &Document) -> usize {
    let text = document.text();
    let mut at = text.len() / 2;
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

fn main() {
    let only = std::env::args().nth(1).filter(|arg| !arg.starts_with('-'));
    let wanted = |name: &str| only.as_deref().is_none_or(|only| name.contains(only));
    let started = Instant::now();
    let base = fixture();
    let counter = base.document.counter();
    println!(
        "fixture: {} words, {} Slots, {} Ghost ranges, {} Lab marks, built in {:?}",
        counter.words,
        base.document.slots().len(),
        base.document.ghost_ranges().len(),
        base.document.lab_marks().len(),
        started.elapsed()
    );

    if wanted("type_char") {
        let mut session = EditSession::new(base.document.clone());
        session.select(Caret::at(middle(&base.document)));
        time("type_char", 500, || {
            let edit = session.caret().insert(session.document().text(), "x");
            assert!(session.edit(&edit));
        });
    }

    if wanted("caret_move") {
        let mut session = EditSession::new(base.document.clone());
        session.select(Caret::at(middle(&base.document)));
        time("caret_move", 2000, || {
            let moved = session
                .caret()
                .moved(session.document().text(), Motion::WordRight);
            session.select(moved);
            black_box(session.counter_label());
        });
    }

    if wanted("undo_redo") {
        let mut session = EditSession::new(base.document.clone());
        session.select(Caret::at(middle(&base.document)));
        for _ in 0..200 {
            let edit = session.caret().insert(session.document().text(), "y");
            assert!(session.edit(&edit));
            session.close_undo_step();
        }
        time("undo_redo (pair)", 200, || {
            assert!(session.undo().is_some());
            assert!(session.redo().is_some());
        });
    }

    if wanted("slot_cycle") {
        let mut session = EditSession::new(base.document.clone());
        let id = base.word_slots[base.word_slots.len() / 2];
        time("slot_cycle", 500, || {
            session
                .change(|document| document.cycle_active(id, Cycle::Next))
                .expect("cycle");
        });
    }

    if wanted("frame_queries") {
        let document = &base.document;
        time("frame_queries", 200, || {
            black_box(document.slots());
            black_box(document.ghost_ranges());
            black_box(document.markdown_styles());
            black_box(document.counter());
        });
    }

    if wanted("save_open") {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("long.md");
        time("save", 20, || base.document.save(&path).expect("save"));
        time("open", 20, || {
            black_box(Document::open(&path).expect("open"));
        });
    }

    if wanted("alternatives_list") {
        let document = &base.document;
        let id = base.word_slots[base.word_slots.len() / 2];
        let range = document.slot(id).expect("Slot").range;
        time("alternatives_list", 2000, || {
            let list = AlternativesList::new(document, range.clone());
            black_box(list.rows(document));
        });
    }

    if wanted("delete_backward") {
        let mut session = EditSession::new(base.document.clone());
        session.select(Caret::at(middle(&base.document)));
        time("delete_backward", 500, || {
            let edit = session
                .caret()
                .delete(session.document().text(), Motion::Left);
            session.edit(&edit);
        });
    }
}
