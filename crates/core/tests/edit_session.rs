use ka_teb_core::{
    Caret, CaretEdit, Document, EditKind, EditSession, Level, Provenance, TrimLevel,
};

#[test]
fn the_target_is_the_selection_else_the_word_at_the_caret_else_the_caret() {
    let mut session = EditSession::new(Document::new("The cat  sat."));
    session.select(Caret::over(0..7));
    assert_eq!(session.target(), 0..7);

    session.select(Caret::at(5));
    assert_eq!(session.target(), 4..7);

    session.select(Caret::at(8));
    assert_eq!(session.target(), 8..8);
}

#[test]
fn the_counter_label_counts_the_document_or_the_selection() {
    let mut session = EditSession::new(Document::new("The cat sat."));
    let document = session.document().counter().to_string();
    assert_eq!(session.counter_label(), document);

    session.select(Caret::over(4..7));
    let selected = session.document().selection_counter(4..7).unwrap();
    assert_eq!(session.counter_label(), format!("{selected} selected"));
}

#[test]
fn typing_in_a_row_is_one_undo_step_and_undo_puts_the_selection_back() {
    let mut session = EditSession::new(Document::new("Hi"));
    session.select(Caret::at(2));

    session.edit(&CaretEdit::new(2..2, " t", EditKind::Typing));
    session.edit(&CaretEdit::new(4..4, "o", EditKind::Typing));
    session.edit(&CaretEdit::new(5..5, "m", EditKind::Typing));
    assert_eq!(session.document().text(), "Hi tom");
    assert_eq!(session.selection(), 6..6);

    session.undo();
    assert_eq!(session.document().text(), "Hi");
    assert_eq!(session.selection(), 2..2);
}

#[test]
fn a_selection_move_starts_a_new_undo_step_and_redo_puts_the_selection_after_it() {
    let mut session = EditSession::new(Document::new(""));
    session.edit(&CaretEdit::new(0..0, "a", EditKind::Typing));
    session.edit(&CaretEdit::new(1..1, "b", EditKind::Typing));
    session.select(Caret::at(1));
    session.edit(&CaretEdit::new(1..1, "c", EditKind::Typing));

    session.undo();
    assert_eq!(session.document().text(), "ab");
    assert_eq!(session.selection(), 1..1);

    session.redo();
    assert_eq!(session.document().text(), "acb");
    assert_eq!(session.selection(), 2..2);
}

#[test]
fn a_change_of_many_core_steps_is_one_undo_step_and_the_selection_follows_the_text() {
    let mut session = EditSession::new(Document::new("The cat sat."));
    session.select(Caret::over(8..12));

    let switched = session.change(|document| {
        let id = document.create_slot(4..7, Level::Word, "old dog", Provenance::Writer)?;
        document.set_active(id, 1)
    });
    assert!(switched.is_ok());
    assert_eq!(session.document().text(), "The old dog sat.");
    assert_eq!(session.selection(), 12..16);

    session.undo();
    assert_eq!(session.document().text(), "The cat sat.");
    assert_eq!(session.document().slots().len(), 0);
    assert_eq!(session.selection(), 8..12);
}

#[test]
fn a_change_that_changes_only_the_trim_proposal_makes_no_undo_step() {
    let mut session = EditSession::new(Document::new("A very long cat."));
    session.edit(&CaretEdit::new(16..16, "!", EditKind::Typing));
    let request = session
        .document()
        .trim_request(TrimLevel::SlightTrim)
        .unwrap();

    session
        .change(|document| document.apply_trim_result(&request, &[2..6, 12..15]))
        .unwrap();
    assert!(session.document().trim_proposal().is_some());

    session.undo();
    assert_eq!(session.document().text(), "A very long cat.");
    assert_eq!(session.selection(), 0..0);
}

#[test]
fn undo_of_a_variant_delete_reports_the_slot_it_puts_back() {
    let mut session = EditSession::new(Document::new("The cat sat."));
    let (id, _) = session
        .change(|document| {
            let id = document.create_slot(4..7, Level::Word, "dog", Provenance::Writer)?;
            document.add_variant(id, "fox", Provenance::Writer)?;
            document.set_active(id, 2).map(|_| id)
        })
        .unwrap();
    session
        .change(|document| document.delete_variant(id, 2))
        .unwrap();
    assert_eq!(session.document().text(), "The cat sat.");

    let undone = session.undo().unwrap();
    assert_eq!(session.document().text(), "The fox sat.");
    assert_eq!(undone.restored, [id]);
    assert_eq!(undone.switched, Some(id));
}

#[test]
fn redo_of_a_variant_delete_reports_the_span_it_deletes_from() {
    let mut session = EditSession::new(Document::new("The cat sat."));
    let (id, _) = session
        .change(|document| {
            let id = document.create_slot(4..7, Level::Word, "dog", Provenance::Writer)?;
            document.add_variant(id, "horse", Provenance::Writer)?;
            document.set_active(id, 2).map(|_| id)
        })
        .unwrap();
    session
        .change(|document| document.delete_variant(id, 2))
        .unwrap();
    session.undo().unwrap();
    assert_eq!(session.document().text(), "The horse sat.");

    let redone = session.redo().unwrap();

    assert_eq!(session.document().text(), "The cat sat.");
    assert_eq!(redone.deleted, vec![4..9_usize]);
}

#[test]
fn undo_and_redo_with_nothing_to_do_report_nothing() {
    let mut session = EditSession::new(Document::new("The cat sat."));

    assert_eq!(session.undo(), None);
    assert_eq!(session.redo(), None);
}

#[test]
fn overflow_edits_and_text_edits_share_one_history_and_keep_the_selection() {
    let mut session = EditSession::new(Document::new("Body."));
    session.edit(&CaretEdit::new(5..5, "!", EditKind::Typing));
    assert!(session.edit_overflow(&CaretEdit::new(0..0, "a", EditKind::Typing)));
    assert!(session.edit_overflow(&CaretEdit::new(1..1, " note", EditKind::Typing)));
    assert_eq!(session.selection(), 6..6);

    session.undo();
    assert_eq!(session.document().overflow().text(), "");
    assert_eq!(session.document().text(), "Body.!");

    session.undo();
    assert_eq!(session.document().text(), "Body.");
}

#[test]
fn a_change_can_place_the_selection_itself_and_undo_puts_it_back() {
    let mut session = EditSession::new(Document::new("Title\nCut this."));
    session.select(Caret::over(6..15));

    session
        .change_then_select(|document| document.stash(6..15), |_, _| 6..6)
        .unwrap();
    assert_eq!(session.document().text(), "Title\n");
    assert_eq!(session.selection(), 6..6);

    session.undo();
    assert_eq!(session.document().text(), "Title\nCut this.");
    assert_eq!(session.selection(), 6..15);
}

#[test]
fn undo_back_to_the_saved_undo_step_is_not_unsaved() {
    let mut session = EditSession::new(Document::new(""));
    assert!(!session.is_unsaved());
    session.edit(&CaretEdit::new(0..0, "a", EditKind::Typing));
    assert!(session.is_unsaved());

    let (saved, at) = session.snapshot();
    session.mark_saved(at);
    assert_eq!(saved.text(), "a");
    assert!(!session.is_unsaved());

    session.edit(&CaretEdit::new(1..1, "b", EditKind::Typing));
    assert!(session.is_unsaved());
    session.undo();
    assert_eq!(session.document().text(), "a");
    assert!(!session.is_unsaved());
}

#[test]
fn a_new_kind_of_edit_or_an_other_edit_starts_a_new_undo_step() {
    let mut session = EditSession::new(Document::new(""));
    session.edit(&CaretEdit::new(0..0, "ab", EditKind::Typing));
    session.edit(&CaretEdit::new(1..2, "", EditKind::Deleting));
    session.edit(&CaretEdit::new(1..1, "x", EditKind::Other));
    session.edit(&CaretEdit::new(2..2, "y", EditKind::Other));

    let mut texts = Vec::new();
    while session.undo().is_some() {
        texts.push(session.document().text().to_owned());
    }
    assert_eq!(texts, ["ax", "a", "ab", ""]);
}

#[test]
fn text_an_input_method_composes_stays_in_one_undo_step() {
    let mut session = EditSession::new(Document::new(""));
    let composed = session.caret().compose("", None, "n", Some(1..1));
    session.edit(&composed);
    let recomposed = session.caret().compose("n", None, "ni", Some(2..2));
    session.edit(&recomposed);
    assert_eq!(session.document().text(), "ni");
    assert_eq!(session.caret().marked(), Some(0..2));

    session.undo();
    assert_eq!(session.document().text(), "");
}
