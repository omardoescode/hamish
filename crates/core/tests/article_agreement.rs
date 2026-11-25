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

#[test]
fn the_article_keeps_its_case() {
    let examples = [
        ("A thumbtack.", 2..11, "eraser", "An eraser."),
        ("An eraser.", 3..9, "thumbtack", "A thumbtack."),
        ("AN ERASER.", 3..9, "THUMBTACK", "A THUMBTACK."),
    ];
    for (text, range, variant, expected) in examples {
        let mut document = Document::new(text);
        let id = document
            .create_slot(range, Level::Word, variant, Provenance::Writer)
            .unwrap();
        document.set_active(id, 1).unwrap();
        assert_eq!(document.text(), expected);
    }
}

#[test]
fn the_article_follows_the_first_sound_not_the_first_letter() {
    let examples = [
        ("an", "hour", "an hour"),
        ("an", "honest", "an honest"),
        ("an", "heir", "an heir"),
        ("an", "university", "a university"),
        ("an", "unicorn", "a unicorn"),
        ("an", "user", "a user"),
        ("an", "European", "a European"),
        ("an", "one-off", "a one-off"),
        ("a", "unimportant", "an unimportant"),
        ("a", "Umbrella", "an Umbrella"),
        ("a", "house", "a house"),
        ("an", "7", "an 7"),
    ];
    for (article, variant, expected) in examples {
        let text = format!("{article} word");
        let start = article.len() + 1;
        let mut document = Document::new(&text);
        let id = document
            .create_slot(start..text.len(), Level::Word, variant, Provenance::Writer)
            .unwrap();
        document.set_active(id, 1).unwrap();
        assert_eq!(document.text(), expected, "{variant}");
    }
}
