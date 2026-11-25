#![expect(
    clippy::single_range_in_vec_init,
    reason = "a list of one Ghost range is the expected value"
)]

use std::error::Error;
use std::fs;
use std::path::Path;

use ka_teb_core::{
    Document, Dropped, LabCheck, LabFinding, Level, Provenance, StorageError, TrimLevel,
    sidecar_path,
};

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

#[test]
fn open_of_an_md_file_with_no_sidecar_gives_a_plain_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.md");
    fs::write(&path, "Written in another tool.").unwrap();

    let opened = Document::open(&path).unwrap();
    assert_eq!(opened.dropped, []);
    assert_eq!(opened.document.text(), "Written in another tool.");
    assert_eq!(opened.document.slots(), []);
    assert_eq!(opened.document.ghost_ranges(), []);
    assert_eq!(opened.document.overflow().text(), "");
}

#[test]
fn open_after_an_outside_edit_finds_items_by_text_and_reports_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let mut document = Document::new("Mind the gap. Then mind the step.");
    document
        .create_slot(9..12, Level::Word, "hole", Provenance::Writer)
        .unwrap();
    document.ghost(14..18).unwrap();
    document.save(&path).unwrap();
    fs::write(&path, "New start. Mind the gap. Mind the step.").unwrap();

    let opened = Document::open(&path).unwrap();
    assert_eq!(
        opened.dropped,
        [Dropped::Ghost {
            text: "Then".to_owned()
        }]
    );
    let document = opened.document;
    assert_eq!(document.text(), "New start. Mind the gap. Mind the step.");
    let slots = document.slots();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].range, 20..23);
    assert_eq!(slots[0].variants[1].text(), "hole");
    assert_eq!(document.ghost_ranges(), []);
}

#[test]
fn an_item_whose_text_shows_more_than_once_goes_to_the_place_nearest_its_old_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let mut document = Document::new("the cat and the dog and the end.");
    document
        .create_slot(12..15, Level::Word, "a", Provenance::Writer)
        .unwrap();
    document
        .create_slot(24..27, Level::Word, "one", Provenance::Writer)
        .unwrap();
    let request = document.lab_request(LabCheck::HedgesFiller);
    document
        .apply_lab_result(
            &request,
            &[LabFinding {
                range: 4..7,
                note: None,
                fix: None,
            }],
        )
        .unwrap();
    document.save(&path).unwrap();
    fs::write(&path, "so the cat and the dog and the end.").unwrap();

    let opened = Document::open(&path).unwrap();
    assert_eq!(opened.dropped, []);
    let document = opened.document;
    let ranges: Vec<_> = document
        .slots()
        .iter()
        .map(|slot| slot.range.clone())
        .collect();
    assert_eq!(ranges, [15..18, 27..30]);
    assert_eq!(document.lab_marks()[0].range, 7..10);
}

#[test]
fn an_item_whose_only_place_is_taken_or_whose_text_is_gone_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let mut document = Document::new("go and go far");
    document
        .create_slot(0..2, Level::Word, "run", Provenance::Writer)
        .unwrap();
    document
        .create_slot(7..9, Level::Word, "walk", Provenance::Ai)
        .unwrap();
    let request = document.lab_request(LabCheck::OffToneWords);
    document
        .apply_lab_result(
            &request,
            &[LabFinding {
                range: 10..13,
                note: Some("vague".to_owned()),
                fix: None,
            }],
        )
        .unwrap();
    document.save(&path).unwrap();
    fs::write(&path, "go now").unwrap();

    let opened = Document::open(&path).unwrap();
    assert_eq!(
        opened.dropped,
        [
            Dropped::Slot {
                level: Level::Word,
                text: "go".to_owned()
            },
            Dropped::LabMark {
                check: LabCheck::OffToneWords,
                text: "far".to_owned()
            },
        ]
    );
    let slots = opened.document.slots();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].range, 0..2);
    assert_eq!(slots[0].variants[1].text(), "run");
    assert_eq!(opened.document.lab_marks(), []);
}

#[test]
fn a_slot_found_again_keeps_the_slots_nested_in_its_active_variant() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let mut document = Document::new("One idea here.");
    document
        .create_slot(0..14, Level::Paragraph, "Another idea.", Provenance::Writer)
        .unwrap();
    document
        .create_slot(4..8, Level::Word, "thought", Provenance::Writer)
        .unwrap();
    document.save(&path).unwrap();
    fs::write(&path, "Intro.\n\nOne idea here.").unwrap();

    let opened = Document::open(&path).unwrap();
    assert_eq!(opened.dropped, []);
    let ranges: Vec<_> = opened
        .document
        .slots()
        .iter()
        .map(|slot| slot.range.clone())
        .collect();
    assert_eq!(ranges, [8..22, 12..16]);
}

#[test]
fn an_opened_document_has_no_edit_history_and_gives_new_slots_new_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    draft().unwrap().save(&path).unwrap();

    let mut document = Document::open(&path).unwrap().document;
    assert!(!document.undo());
    let old: Vec<_> = document.slots().iter().map(|slot| slot.id).collect();
    let new = document
        .create_slot(0..3, Level::Word, "A", Provenance::Writer)
        .unwrap();
    assert!(!old.contains(&new));
}

#[test]
fn the_sidecar_sits_next_to_the_md_file() {
    assert_eq!(
        sidecar_path(Path::new("/notes/draft.md")),
        Path::new("/notes/draft.md.ka-teb.json")
    );
}

#[test]
fn open_of_a_missing_md_file_returns_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let error = Document::open(&dir.path().join("none.md")).unwrap_err();
    assert!(matches!(error, StorageError::Io { .. }));
}

#[test]
fn save_into_a_missing_folder_returns_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let error = Document::new("text")
        .save(&dir.path().join("none").join("draft.md"))
        .unwrap_err();
    assert!(matches!(error, StorageError::Io { .. }));
}

#[test]
fn open_of_an_md_file_that_is_not_utf8_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    fs::write(&path, [0x66, 0xff, 0x6f]).unwrap();
    assert!(matches!(
        Document::open(&path),
        Err(StorageError::NotUtf8(_))
    ));
}

#[test]
fn open_with_a_sidecar_that_is_not_sidecar_json_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    Document::new("text").save(&path).unwrap();
    fs::write(sidecar_path(&path), "{ not json").unwrap();
    assert!(matches!(
        Document::open(&path),
        Err(StorageError::BadSidecar { .. })
    ));
}

#[test]
fn open_with_a_sidecar_of_a_newer_schema_version_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    fs::write(&path, "text").unwrap();
    fs::write(
        sidecar_path(&path),
        r#"{"version": 2, "slots": "new shape"}"#,
    )
    .unwrap();
    assert!(matches!(
        Document::open(&path),
        Err(StorageError::UnknownVersion { version: 2, .. })
    ));
}

#[test]
fn a_slot_the_sidecar_gives_no_active_variant_is_dropped_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let mut document = Document::new("Pick one word.");
    document
        .create_slot(5..8, Level::Word, "two", Provenance::Writer)
        .unwrap();
    document.save(&path).unwrap();
    let sidecar = fs::read_to_string(sidecar_path(&path)).unwrap();
    fs::write(
        sidecar_path(&path),
        sidecar.replace("\"active\": 0", "\"active\": 7"),
    )
    .unwrap();

    let opened = Document::open(&path).unwrap();
    assert_eq!(
        opened.dropped,
        [Dropped::Slot {
            level: Level::Word,
            text: String::new()
        }]
    );
    assert_eq!(opened.document.text(), "Pick one word.");
    assert_eq!(opened.document.slots(), []);
}

#[test]
fn an_open_trim_proposal_is_not_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let mut document = Document::new("The cat sat. It was very, very good. The end.");
    let request = document.trim_request(TrimLevel::TightenMore).unwrap();
    document.apply_trim_result(&request, &[13..20]).unwrap();
    document.save(&path).unwrap();

    let opened = Document::open(&path).unwrap().document;

    assert_eq!(opened.trim_proposal(), None);
    assert_eq!(
        opened.text(),
        "The cat sat. It was very, very good. The end."
    );
}
