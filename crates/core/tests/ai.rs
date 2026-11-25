use ka_teb_core::{AiAdapter, AiError, AiRequest, Document, Level, Provenance, SlotId};

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
