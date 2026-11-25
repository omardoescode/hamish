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

#[test]
fn undo_restores_both_the_variant_and_the_article() {
    let mut document = Document::new("Hand me a thumbtack.");
    let id = document
        .create_slot(10..19, Level::Word, "eraser", Provenance::Writer)
        .unwrap();
    document.set_active(id, 1).unwrap();
    assert!(document.undo());
    assert_eq!(document.text(), "Hand me a thumbtack.");
    let slot = document.slot(id).unwrap();
    assert_eq!((slot.range.clone(), slot.active), (10..19, 0));
    assert!(document.redo());
    assert_eq!(document.text(), "Hand me an eraser.");
    assert_eq!(document.slot(id).unwrap().range, 11..17);
}

#[test]
fn only_a_variant_change_of_a_word_slot_moves_the_article() {
    let mut document = Document::new("a eraser. a apple.");
    let word = document
        .create_slot(2..8, Level::Word, "thumbtack", Provenance::Writer)
        .unwrap();
    assert_eq!(document.text(), "a eraser. a apple.");
    document
        .add_variant(word, "owl", Provenance::Writer)
        .unwrap();
    assert_eq!(document.text(), "a eraser. a apple.");
    document.replace(12..17, "egg").unwrap();
    assert_eq!(document.text(), "a eraser. a egg.");
    let sentence = document
        .create_slot(10..16, Level::Sentence, "a idea.", Provenance::Writer)
        .unwrap();
    document.set_active(sentence, 1).unwrap();
    assert_eq!(document.text(), "a eraser. a idea.");
}

#[test]
fn an_article_inside_a_word_slot_of_its_own_stays() {
    let mut document = Document::new("a thumbtack");
    let article = document
        .create_slot(0..1, Level::Word, "the", Provenance::Writer)
        .unwrap();
    let noun = document
        .create_slot(2..11, Level::Word, "eraser", Provenance::Writer)
        .unwrap();
    document.set_active(noun, 1).unwrap();
    assert_eq!(document.text(), "a eraser");
    assert_eq!(document.slot(article).unwrap().range, 0..1);
}

#[test]
fn a_word_slot_nested_in_a_sentence_slot_moves_the_article_inside_it() {
    let mut document = Document::new("I want a thumbtack. Thanks.");
    let sentence = document
        .create_slot(0..19, Level::Sentence, "No.", Provenance::Writer)
        .unwrap();
    let word = document
        .create_slot(9..18, Level::Word, "eraser", Provenance::Writer)
        .unwrap();
    document.set_active(word, 1).unwrap();
    assert_eq!(document.text(), "I want an eraser. Thanks.");
    assert_eq!(document.slot(sentence).unwrap().range, 0..17);
    assert_eq!(document.slot(word).unwrap().range, 10..16);
}

#[test]
fn editing_the_active_variant_and_ending_the_slot_also_agree() {
    let mut document = Document::new("a thumbtack");
    let id = document
        .create_slot(2..11, Level::Word, "eraser", Provenance::Writer)
        .unwrap();
    document.edit_variant(id, 0, "apple").unwrap();
    assert_eq!(document.text(), "an apple");
    document.set_active(id, 1).unwrap();
    document.delete_variant(id, 1).unwrap();
    assert_eq!(document.text(), "an apple");
    assert!(document.slot(id).is_none());
}

#[test]
fn an_initialism_takes_the_article_of_its_first_letter_name() {
    let examples = [
        ("a", "FBI", "an FBI"),
        ("a", "MP3", "an MP3"),
        ("an", "UFO", "a UFO"),
        ("an", "BBC", "a BBC"),
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
