#![expect(
    clippy::single_range_in_vec_init,
    reason = "a list of one Ghost range is the expected value"
)]

use std::error::Error;
use std::fs;

use ka_teb_core::{Document, LabCheck, LabFinding, Level, Provenance};

/// "The rapidly fox runs.\n\nAnother paragraph." with a Word Slot (Mixed Variant active),
/// a Paragraph Slot whose inactive Original holds a nested Slot and a Ghost range,
/// a top-level Ghost range, Overflow text, and an open Lab mark.
fn draft() -> Result<Document, Box<dyn Error>> {
    let mut document = Document::new("The quick fox runs.\n\nA second paragraph here.");
    let word = document.create_slot(4..9, Level::Word, "swift", Provenance::Writer)?;
    let paragraph = document.create_slot(
        21..45,
        Level::Paragraph,
        "Another paragraph.",
        Provenance::Ai,
    )?;
    document.create_slot(23..29, Level::Word, "next", Provenance::Writer)?;
    document.ghost(40..44)?;
    document.set_active(paragraph, 1)?;
    document.add_variant(word, "rapid", Provenance::Ai)?;
    document.edit_variant(word, 2, "rapidly")?;
    document.set_active(word, 2)?;
    document.ghost(12..15)?;
    document.edit_overflow(0..0, "kept line\n\nsecond block")?;
    let request = document.lab_request(LabCheck::LongSentences);
    document.apply_lab_result(
        &request,
        &[LabFinding {
            range: 16..20,
            note: Some("long".to_owned()),
            fix: None,
        }],
    )?;
    Ok(document)
}

#[test]
fn save_writes_only_the_active_text_to_the_md_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    draft().unwrap().save(&path).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "The rapidly fox runs.\n\nAnother paragraph."
    );
}

#[test]
fn open_after_save_gives_the_same_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let saved = draft().unwrap();
    saved.save(&path).unwrap();

    let opened = Document::open(&path).unwrap();
    assert_eq!(opened.dropped, []);
    let document = opened.document;
    assert_eq!(
        document.text(),
        "The rapidly fox runs.\n\nAnother paragraph."
    );
    assert_eq!(document.slots(), saved.slots());
    assert_eq!(document.ghost_ranges(), [12..15]);
    assert_eq!(document.overflow().text(), "kept line\n\nsecond block");
    assert_eq!(document.lab_marks(), saved.lab_marks());
    let word = &document.slots()[0];
    assert_eq!(word.active, 2);
    assert_eq!(word.variants[2].provenance(), Provenance::Mixed);
    assert_eq!(document.slots()[1].variants[1].provenance(), Provenance::Ai);
}

#[test]
fn nested_slots_and_ghost_ranges_of_an_inactive_variant_survive_the_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    draft().unwrap().save(&path).unwrap();

    let mut document = Document::open(&path).unwrap().document;
    let paragraph = document.slots()[1].id;
    document.set_active(paragraph, 0).unwrap();
    assert_eq!(
        document.text(),
        "The rapidly fox runs.\n\nA second paragraph here."
    );
    let nested = &document.slots()[2];
    assert_eq!(nested.range, 25..31);
    assert_eq!(nested.variants[1].text(), "next");
    assert_eq!(document.ghost_ranges(), [12..15, 42..46]);
}
