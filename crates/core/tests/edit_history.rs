use ka_teb_core::{EditHistory, EditKind};

#[test]
fn typing_at_the_cursor_joins_into_one_undo_step() {
    let mut history = EditHistory::default();
    history.record(EditKind::Typing, 0..0, 1..1);
    history.record(EditKind::Typing, 1..1, 2..2);
    history.record(EditKind::Typing, 2..2, 3..3);

    let group = history.undo().unwrap();

    assert_eq!((group.steps, group.before, group.after), (3, 0..0, 3..3));
    assert_eq!(history.undo(), None);
}

#[test]
fn a_different_kind_a_jump_or_a_close_starts_a_new_undo_step() {
    let mut history = EditHistory::default();
    history.record(EditKind::Typing, 0..0, 1..1);
    history.record(EditKind::Deleting, 1..1, 0..0);
    history.record(EditKind::Deleting, 5..5, 4..4);
    history.close();
    history.record(EditKind::Deleting, 4..4, 3..3);

    let steps: Vec<usize> =
        std::iter::from_fn(|| history.undo().map(|group| group.steps)).collect();

    assert_eq!(steps, [1, 1, 1, 1]);
}

#[test]
fn other_edits_never_join() {
    let mut history = EditHistory::default();
    history.record(EditKind::Other, 0..0, 1..1);
    history.record(EditKind::Other, 1..1, 2..2);

    assert_eq!(history.undo().map(|group| group.steps), Some(1));
    assert_eq!(history.undo().map(|group| group.steps), Some(1));
}

#[test]
fn record_steps_makes_one_undo_step_of_many_core_steps() {
    let mut history = EditHistory::default();
    history.record(EditKind::Typing, 0..0, 1..1);
    history.record_steps(2, 1..1, 4..4);
    history.record(EditKind::Typing, 4..4, 5..5);

    assert_eq!(history.undo().map(|group| group.steps), Some(1));
    assert_eq!(history.undo().map(|group| group.steps), Some(2));
}

#[test]
fn undo_then_redo_gives_back_the_same_step_and_a_new_edit_clears_redo() {
    let mut history = EditHistory::default();
    history.record(EditKind::Typing, 0..0, 1..1);
    let saved = history.top();

    let undone = history.undo().unwrap();
    assert_eq!(history.top(), None);
    assert_eq!(history.redo(), Some(undone));
    assert_eq!(history.top(), saved);

    history.undo();
    history.record(EditKind::Other, 0..0, 2..2);
    assert_eq!(history.redo(), None);
    assert_ne!(history.top(), saved);
}
