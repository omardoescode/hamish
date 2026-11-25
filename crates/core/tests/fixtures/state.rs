use ka_teb_core::{Document, SlotId};

/// Variant texts of a Slot, the Original first; empty when the Slot does not show.
pub fn variant_texts(document: &Document, slot: SlotId) -> Vec<String> {
    document.slot(slot).map_or_else(Vec::new, |slot| {
        slot.variants
            .iter()
            .map(|variant| variant.text().to_owned())
            .collect()
    })
}

/// Active variant index and Variant texts of one visible Slot.
pub type SlotState<'a> = (usize, &'a [&'a str]);

fn state(document: &Document) -> (String, Vec<(usize, Vec<String>)>) {
    let slots = document
        .slots()
        .iter()
        .map(|slot| {
            let texts = slot.variants.iter().map(|v| v.text().to_owned()).collect();
            (slot.active, texts)
        })
        .collect();
    (document.text().to_owned(), slots)
}

/// Asserts the Document text and, in order, each visible Slot's state.
pub fn expect_state(document: &Document, text: &str, slots: &[SlotState<'_>]) {
    let expected = slots
        .iter()
        .map(|(active, texts)| (*active, texts.iter().map(|t| (*t).to_owned()).collect()))
        .collect();
    assert_eq!(state(document), (text.to_owned(), expected));
}
