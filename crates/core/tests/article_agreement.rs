use ka_teb_core::{Document, Level, Provenance};

#[test]
fn switching_a_word_slot_makes_the_article_agree() {
    let mut document = Document::new("Hand me a thumbtack, please.");
    let id = document
        .create_slot(10..19, Level::Word, "eraser", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    assert_eq!(document.text(), "Hand me an eraser, please.");
    assert_eq!(document.slot(id).unwrap().range, 11..17);
    document.set_active(id, 0).unwrap();
    assert_eq!(document.text(), "Hand me a thumbtack, please.");
    assert_eq!(document.slot(id).unwrap().range, 10..19);
}
