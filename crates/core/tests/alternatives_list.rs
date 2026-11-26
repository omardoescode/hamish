use hamish_core::{
    AiTrigger, AlternativesList, Caret, Document, EditSession, Level, ListOutcome, Provenance,
    Switch,
};

/// "Title\nThe customer waits." ("customer" is 10..18).
fn customer_session() -> EditSession {
    EditSession::new(Document::new("Title\nThe customer waits."))
}

/// "customer" as a Word Slot: "customer", "client", "patron" (AI); "customer" shows.
#[expect(clippy::unwrap_used, reason = "test fixture")]
fn slot_session() -> EditSession {
    let mut document = Document::new("Title\nThe customer waits.");
    let slot = document
        .create_slot(10..18, Level::Word, "client", Provenance::Writer)
        .unwrap();
    document
        .add_variant(slot, "patron", Provenance::Ai)
        .unwrap();
    EditSession::new(document)
}

fn rows(list: &AlternativesList, session: &EditSession) -> Vec<(String, bool)> {
    list.rows(session.document())
        .into_iter()
        .map(|row| (row.text, row.active))
        .collect()
}

fn row(text: &str, active: bool) -> (String, bool) {
    (text.to_owned(), active)
}

fn add(list: &mut AlternativesList, session: &mut EditSession, text: &str) -> ListOutcome {
    list.type_text(session.document(), text);
    list.confirm(session)
}

#[test]
fn return_in_the_last_row_adds_the_draft_as_the_active_variant_and_a_new_empty_row() {
    let mut session = customer_session();
    let mut list = AlternativesList::new(session.document(), 10..18);

    let outcome = add(&mut list, &mut session, "client");
    assert_eq!(outcome.switch, Some(Switch::ToVariant));
    add(&mut list, &mut session, "patron");

    assert_eq!(
        rows(&list, &session),
        vec![
            row("customer", false),
            row("client", false),
            row("patron", true)
        ]
    );
    assert_eq!(list.row(), 3);
    assert_eq!(list.draft(), "");
    assert_eq!(session.document().text(), "Title\nThe patron waits.");
}

#[test]
fn one_undo_removes_a_variant_added_with_return() {
    let mut session = customer_session();
    let mut list = AlternativesList::new(session.document(), 10..18);
    add(&mut list, &mut session, "client");
    add(&mut list, &mut session, "patron");

    session.undo();

    assert_eq!(session.document().text(), "Title\nThe client waits.");
    assert_eq!(
        session
            .document()
            .slots()
            .first()
            .map(|slot| slot.variants.len()),
        Some(2)
    );
}

#[test]
fn the_draft_shows_in_place_of_the_target_until_it_starts_a_quick_fill() {
    let session = slot_session();
    let mut list = AlternativesList::new(session.document(), 10..18);

    let outcome = list.type_text(session.document(), "buyer\ns");
    assert!(outcome.preview && outcome.scroll && !outcome.quick_fill);
    assert_eq!(list.preview(session.document()), Some((10..18, "buyer s")));

    let mut list = AlternativesList::new(session.document(), 10..18);
    list.type_text(session.document(), "?");
    let outcome = list.type_text(session.document(), "?");
    assert!(outcome.quick_fill);
    assert_eq!(list.draft(), "");
    assert_eq!(list.preview(session.document()), None);
}

#[test]
fn return_on_a_variant_edits_it_and_an_edited_ai_variant_is_mixed() {
    let mut session = slot_session();
    let mut list = AlternativesList::new(session.document(), 10..18);
    list.select(&mut session, 2);

    list.confirm(&mut session);
    assert_eq!(list.editing(), Some("patron"));
    for _ in 0..4 {
        list.delete_backward();
    }
    list.type_text(session.document(), "rty");
    list.confirm(&mut session);

    assert_eq!(session.document().text(), "Title\nThe party waits.");
    let provenances: Vec<Provenance> = list
        .rows(session.document())
        .into_iter()
        .map(|row| row.provenance)
        .collect();
    assert_eq!(
        provenances,
        vec![Provenance::Writer, Provenance::Writer, Provenance::Mixed]
    );
}

#[test]
fn escape_while_editing_drops_the_edit_and_the_next_escape_closes() {
    let mut session = slot_session();
    let mut list = AlternativesList::new(session.document(), 10..18);
    list.select(&mut session, 2);
    list.confirm(&mut session);
    list.delete_backward();

    assert!(!list.escape());
    assert_eq!(list.editing(), None);
    assert_eq!(session.document().text(), "Title\nThe patron waits.");
    assert!(list.escape());
}

#[test]
fn deleting_all_but_one_variant_ends_the_slot_and_the_list_keeps_its_text() {
    let mut session = slot_session();
    let mut list = AlternativesList::new(session.document(), 10..18);
    list.select(&mut session, 2);
    assert_eq!(list.delete_variant(&mut session), Some(10..16));
    list.select(&mut session, 1);
    assert_eq!(list.delete_variant(&mut session), Some(10..16));

    assert_eq!(rows(&list, &session), vec![row("customer", true)]);
    assert_eq!(session.document().slots().len(), 0);

    add(&mut list, &mut session, "buyer");
    assert_eq!(
        rows(&list, &session),
        vec![row("customer", false), row("buyer", true)]
    );
}

#[test]
fn a_text_target_follows_edits_before_it_and_text_edited_away_leaves_none() {
    let mut document = Document::new("Title\nThe customer waits.");
    let mut list = AlternativesList::new(&document, 10..18);

    document.insert(2, "xy").unwrap();
    assert!(list.follow_document(&document));
    assert_eq!(rows_of(&list, &document), vec![row("customer", true)]);

    document.delete(10..22).unwrap();
    assert!(list.follow_document(&document));
    assert_eq!(rows_of(&list, &document), vec![]);
    assert_eq!(list.level(&document), None);
}

fn rows_of(list: &AlternativesList, document: &Document) -> Vec<(String, bool)> {
    list.rows(document)
        .into_iter()
        .map(|row| (row.text, row.active))
        .collect()
}

#[test]
fn a_level_shows_the_text_around_the_target_or_the_slot_of_that_range_and_level() {
    let mut document = Document::new("Title\nThe customer waits. She leaves.\nNext.");
    let sentence = document
        .create_slot(6..25, Level::Sentence, "A buyer waits.", Provenance::Writer)
        .unwrap();
    let mut list = AlternativesList::new(&document, 30..36);

    list.choose_level(&document, Level::Paragraph);
    assert_eq!(
        rows_of(&list, &document),
        vec![row("The customer waits. She leaves.", true)]
    );
    assert_eq!(list.level(&document), Some(Level::Paragraph));

    list.choose_level(&document, Level::Word);
    assert_eq!(rows_of(&list, &document), vec![row("The", true)]);

    list.choose_level(&document, Level::Sentence);
    assert_eq!(list.target(), &hamish_core::ListTarget::Slot(sentence));
}

#[test]
fn after_an_undo_that_removes_the_slot_the_list_shows_the_text_at_the_selection() {
    let mut session = customer_session();
    session.select(Caret::over(10..18));
    let mut list = AlternativesList::new(session.document(), session.target());
    add(&mut list, &mut session, "client");

    session.undo();
    list.after_undo_step(session.document(), session.target());

    assert_eq!(rows(&list, &session), vec![row("customer", true)]);
    assert_eq!(list.row(), 1);
}

#[test]
fn quick_fill_results_show_two_at_a_time_then_the_active_row_is_selected_and_the_last_row_hides() {
    let mut session = slot_session();
    let mut list = AlternativesList::new(session.document(), 10..18);
    let request = list
        .ai_request(session.document(), AiTrigger::QuickFill)
        .unwrap();
    let texts: Vec<String> = ["buyer", "guest", "shopper"].map(str::to_owned).into();

    list.answered(&mut session, &request, &texts).unwrap();
    assert!(list.is_revealing());
    assert_eq!(list.rows(session.document()).len(), 3);

    assert!(list.reveal_batch(session.document()));
    assert_eq!(list.rows(session.document()).len(), 5);
    assert!(!list.reveal_batch(session.document()));

    assert!(!list.is_revealing());
    assert_eq!(list.rows(session.document()).len(), 6);
    assert_eq!(list.row(), 0);
    assert!(list.new_row_hidden());
    list.select(&mut session, 9);
    assert!(!list.new_row_hidden());
}

#[test]
fn start_edit_edits_the_variant_row_and_never_the_last_row() {
    let mut session = slot_session();
    let mut list = AlternativesList::new(session.document(), 10..18);

    list.start_edit(session.document());
    assert_eq!(list.editing(), None);

    list.select(&mut session, 1);
    list.start_edit(session.document());
    list.type_text(session.document(), "s");
    list.start_edit(session.document());
    assert_eq!(list.editing(), Some("clients"));
}

#[test]
fn start_edit_on_text_with_no_slot_edits_nothing() {
    let mut session = customer_session();
    let mut list = AlternativesList::new(session.document(), 10..18);
    list.select(&mut session, 0);

    list.start_edit(session.document());

    assert_eq!(list.editing(), None);
}
