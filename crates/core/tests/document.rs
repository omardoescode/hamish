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

#[test]
fn counter_counts_words_and_chars_of_the_document() {
    let examples = [
        ("", "0 words 0 chars"),
        ("I", "1 word 1 char"),
        ("Hi", "1 word 2 chars"),
        ("a b", "2 words 3 chars"),
        ("Hello, dear world", "3 words 17 chars"),
        ("كاتب يكتب", "2 words 9 chars"),
        ("Title\n\nOne two.  Three", "4 words 22 chars"),
    ];
    for (text, expected) in examples {
        assert_eq!(
            Document::new(text).counter().to_string(),
            expected,
            "{text:?}"
        );
    }
}

#[test]
fn selection_counter_counts_only_the_selected_text() {
    let document = Document::new("Hello, dear world");
    assert_eq!(
        document.selection_counter(7..11).unwrap().to_string(),
        "1 word 4 chars"
    );
    assert_eq!(
        document.selection_counter(2..9).unwrap().to_string(),
        "2 words 7 chars"
    );
    assert_eq!(
        document.selection_counter(5..5).unwrap().to_string(),
        "0 words 0 chars"
    );
}

#[test]
fn selection_counter_rejects_a_range_outside_the_text() {
    let document = Document::new("Hello");
    assert!(document.selection_counter(2..9).is_err());
}

#[test]
fn title_is_the_first_line_when_it_has_text() {
    let examples = [
        ("My draft\nThe body starts here.", Some("My draft")),
        ("One line only", Some("One line only")),
        ("Windows title\r\nBody", Some("Windows title")),
        ("", None),
        ("\nBody after an empty first line", None),
        ("   \nBody after a blank first line", None),
    ];
    for (text, expected) in examples {
        assert_eq!(Document::new(text).title(), expected, "{text:?}");
    }
}

#[test]
fn title_follows_edits_to_the_first_line() {
    let mut document = Document::new("\nBody");
    document.insert(0, "Late title").unwrap();
    assert_eq!(document.title(), Some("Late title"));
}

#[test]
fn undo_reverts_one_edit_and_redo_reapplies_it() {
    let mut document = Document::new("The cat sat.");
    document.replace(4..7, "dog").unwrap();
    assert!(document.undo());
    assert_eq!(document.text(), "The cat sat.");
    assert!(document.redo());
    assert_eq!(document.text(), "The dog sat.");
}

#[test]
fn undo_and_redo_walk_several_steps_in_order() {
    let mut document = Document::new("one");
    document.insert(3, " two").unwrap();
    document.insert(7, " three").unwrap();
    document.delete(0..4).unwrap();
    assert_eq!(document.text(), "two three");

    assert!(document.undo());
    assert_eq!(document.text(), "one two three");
    assert!(document.undo());
    assert_eq!(document.text(), "one two");
    assert!(document.undo());
    assert_eq!(document.text(), "one");

    assert!(document.redo());
    assert_eq!(document.text(), "one two");
    assert!(document.redo());
    assert_eq!(document.text(), "one two three");
}

#[test]
fn a_new_edit_after_undo_clears_redo() {
    let mut document = Document::new("draft");
    document.insert(5, " one").unwrap();
    assert!(document.undo());
    document.insert(5, " two").unwrap();
    assert!(!document.redo());
    assert_eq!(document.text(), "draft two");
    assert!(document.undo());
    assert_eq!(document.text(), "draft");
}

#[test]
fn undo_and_redo_with_no_steps_change_nothing() {
    let mut document = Document::new("كاتب");
    assert!(document.insert(1, "x").is_err());
    document.insert(2, "").unwrap();
    assert!(!document.undo());
    assert!(!document.redo());
    assert_eq!(document.text(), "كاتب");
}

#[test]
fn undo_restores_the_counter_and_title() {
    let mut document = Document::new("Title\nBody");
    document.delete(0..6).unwrap();
    assert_eq!(document.title(), Some("Body"));
    assert!(document.undo());
    assert_eq!(document.title(), Some("Title"));
    assert_eq!(document.counter().to_string(), "2 words 10 chars");
}
