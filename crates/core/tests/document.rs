use ka_teb_core::Document;

#[test]
fn insert_text_reads_back_as_active_text() {
    let mut document = Document::new("Hello world");
    document.insert(5, ", dear").unwrap();
    assert_eq!(document.text(), "Hello, dear world");
}

#[test]
fn delete_range_removes_its_text() {
    let mut document = Document::new("Hello, dear world");
    document.delete(5..11).unwrap();
    assert_eq!(document.text(), "Hello world");
}

#[test]
fn replace_range_swaps_its_text() {
    let mut document = Document::new("The cat sat.");
    document.replace(4..7, "dog").unwrap();
    assert_eq!(document.text(), "The dog sat.");
}

#[test]
fn edit_outside_text_or_inside_a_char_is_rejected_and_changes_nothing() {
    let mut document = Document::new("كاتب");
    assert!(document.insert(9, "x").is_err());
    assert!(document.delete(1..4).is_err());
    let reversed = std::ops::Range { start: 4, end: 2 };
    assert!(document.replace(reversed, "x").is_err());
    assert_eq!(document.text(), "كاتب");
}
