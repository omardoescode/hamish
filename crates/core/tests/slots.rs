use ka_teb_core::{Document, Level, Provenance};

fn variant_texts(document: &Document, slot: ka_teb_core::SlotId) -> Vec<String> {
    document.slot(slot).map_or_else(Vec::new, |slot| {
        slot.variants
            .iter()
            .map(|variant| variant.text().to_owned())
            .collect()
    })
}

#[test]
fn create_slot_at_each_level_keeps_the_range_text_as_the_original() {
    let text = "The cat sat. It purred.\n\nA second paragraph.";
    let examples = [
        (4..7, Level::Word, "cat"),
        (13..23, Level::Sentence, "It purred."),
        (25..44, Level::Paragraph, "A second paragraph."),
    ];
    for (range, level, original) in examples {
        let mut document = Document::new(text);
        let id = document
            .create_slot(range.clone(), level, "new text", Provenance::Writer)
            .unwrap();
        let slot = document.slot(id).unwrap();
        assert_eq!(slot.level, level);
        assert_eq!(slot.range, range);
        assert_eq!(slot.active, 0);
        assert_eq!(slot.variants[0].provenance(), Provenance::Writer);
        assert_eq!(slot.variants[1].provenance(), Provenance::Writer);
        assert_eq!(variant_texts(&document, id), [original, "new text"]);
        assert_eq!(document.text(), text);
    }
}
