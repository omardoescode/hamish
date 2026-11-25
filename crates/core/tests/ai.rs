use ka_teb_core::{AiAdapter, AiError, AiRequest, ApplyError, Document, Level, Provenance, SlotId};

/// Returns fixed Variant texts, as an AI backend would.
struct FakeAdapter(Vec<&'static str>);

impl AiAdapter for FakeAdapter {
    fn variants(&self, _request: &AiRequest) -> Result<Vec<String>, AiError> {
        Ok(self.0.iter().map(|text| (*text).to_owned()).collect())
    }
}

fn variants(document: &Document, slot: SlotId) -> Vec<(String, Provenance)> {
    document.slot(slot).map_or_else(Vec::new, |slot| {
        slot.variants
            .iter()
            .map(|variant| (variant.text().to_owned(), variant.provenance()))
            .collect()
    })
}

fn owned(pairs: &[(&str, Provenance)]) -> Vec<(String, Provenance)> {
    pairs
        .iter()
        .map(|(text, provenance)| ((*text).to_owned(), *provenance))
        .collect()
}

#[test]
fn ai_result_makes_a_slot_of_ai_variants_and_keeps_the_original_active() {
    let mut document = Document::new("The cat sat.");
    let request = document.ai_request(4..7, Level::Word).unwrap();
    assert_eq!(request.selection(), "cat");
    assert_eq!(request.context(), "The cat sat.");
    let texts = FakeAdapter(vec!["dog", "fox"]).variants(&request).unwrap();

    let id = document.apply_ai_result(&request, &texts).unwrap();

    assert_eq!(document.text(), "The cat sat.");
    let slot = document.slot(id).unwrap();
    assert_eq!(slot.level, Level::Word);
    assert_eq!(slot.range, 4..7);
    assert_eq!(slot.active, 0);
    assert_eq!(
        variants(&document, id),
        owned(&[
            ("cat", Provenance::Writer),
            ("dog", Provenance::Ai),
            ("fox", Provenance::Ai),
        ])
    );
}

#[test]
fn ai_result_on_an_existing_slot_adds_variants_and_keeps_the_active_one() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    let request = document.ai_request(4..7, Level::Word).unwrap();
    let texts = FakeAdapter(vec!["fox", "owl"]).variants(&request).unwrap();

    assert_eq!(document.apply_ai_result(&request, &texts), Ok(id));

    assert_eq!(document.text(), "The dog sat.");
    assert_eq!(document.slot(id).unwrap().active, 1);
    assert_eq!(
        variants(&document, id),
        owned(&[
            ("cat", Provenance::Writer),
            ("dog", Provenance::Writer),
            ("fox", Provenance::Ai),
            ("owl", Provenance::Ai),
        ])
    );
}

#[test]
fn undo_removes_the_whole_ai_result_in_one_step() {
    let mut document = Document::new("The cat sat.");
    let id = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    let request = document.ai_request(4..7, Level::Word).unwrap();
    document
        .apply_ai_result(&request, &["fox".to_owned(), "owl".to_owned()])
        .unwrap();

    assert!(document.undo());
    assert_eq!(
        variants(&document, id),
        owned(&[("cat", Provenance::Writer), ("dog", Provenance::Writer)])
    );
    assert!(document.undo());
    assert_eq!(document.slots(), []);
    assert!(!document.undo());

    assert!(document.redo());
    assert!(document.redo());
    assert_eq!(variants(&document, id).len(), 4);
}

#[test]
fn undo_removes_a_slot_the_ai_result_made() {
    let mut document = Document::new("The cat sat.");
    let request = document.ai_request(4..7, Level::Word).unwrap();
    document
        .apply_ai_result(&request, &["dog".to_owned(), "fox".to_owned()])
        .unwrap();

    assert!(document.undo());
    assert_eq!(document.slots(), []);
    assert_eq!(document.text(), "The cat sat.");
    assert!(!document.undo());
}

#[test]
fn ai_result_for_text_that_changed_since_the_request_is_refused() {
    let mut document = Document::new("The cat sat.");
    let request = document.ai_request(4..7, Level::Word).unwrap();
    document.replace(4..7, "dog").unwrap();

    assert_eq!(
        document.apply_ai_result(&request, &["fox".to_owned()]),
        Err(ApplyError::Stale)
    );
    assert_eq!(document.slots(), []);
    assert!(document.undo());
    assert!(!document.undo());
}

#[test]
fn ai_texts_that_are_blank_or_repeat_a_variant_are_dropped() {
    let mut document = Document::new("The cat sat.");
    let request = document.ai_request(4..7, Level::Word).unwrap();
    let texts = ["cat", "dog", "  ", "dog", "fox"].map(str::to_owned);

    let id = document.apply_ai_result(&request, &texts).unwrap();

    assert_eq!(
        variants(&document, id),
        owned(&[
            ("cat", Provenance::Writer),
            ("dog", Provenance::Ai),
            ("fox", Provenance::Ai),
        ])
    );
}

#[test]
fn ai_result_with_no_new_text_is_refused_and_changes_nothing() {
    let mut document = Document::new("The cat sat.");
    let request = document.ai_request(4..7, Level::Word).unwrap();

    for texts in [vec![], vec!["cat".to_owned(), String::new()]] {
        assert_eq!(
            document.apply_ai_result(&request, &texts),
            Err(ApplyError::NoVariants)
        );
    }
    assert_eq!(document.slots(), []);
    assert!(!document.undo());
}
