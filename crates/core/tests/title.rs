use std::path::Path;

use ka_teb_core::{Document, document_title};

#[test]
fn a_saved_document_shows_its_file_name_without_md() {
    let document = Document::new("First line\nBody");

    assert_eq!(
        document_title(Some(Path::new("/home/w/drafts/letter.md")), &document),
        "letter"
    );
}

#[test]
fn an_unsaved_document_shows_its_first_line() {
    let document = Document::new("Ναυτίλος\nBody");

    assert_eq!(document_title(None, &document), "Ναυτίλος");
}

#[test]
fn an_unsaved_document_title_drops_the_markdown_marks() {
    let heading = Document::new("# Spring notes\nBody");
    let styled = Document::new("## A *quiet* `walk`\nBody");

    assert_eq!(document_title(None, &heading), "Spring notes");
    assert_eq!(document_title(None, &styled), "A quiet walk");
}

#[test]
fn an_unsaved_document_with_only_marks_on_the_first_line_is_untitled() {
    assert_eq!(document_title(None, &Document::new("#\nBody")), "Untitled");
}

#[test]
fn an_unsaved_document_with_a_blank_first_line_is_untitled() {
    assert_eq!(document_title(None, &Document::new("  \nBody")), "Untitled");
    assert_eq!(document_title(None, &Document::default()), "Untitled");
}
