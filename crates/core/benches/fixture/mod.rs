//! A long Document for the benches: about 50k words with Markdown, nested Slots, Ghost
//! ranges, and Lab marks.

use std::ops::Range;

use hamish_core::{Document, LabCheck, LabFinding, Level, Provenance, SlotId};

const WORDS: [&str; 12] = [
    "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet",
    "kilo", "lima",
];
const PARAGRAPHS: usize = 500;
const WORDS_PER_PARAGRAPH: usize = 100;

pub struct Fixture {
    pub document: Document,
    pub word_slots: Vec<SlotId>,
}

/// Byte ranges of the words, the sentences, and the paragraphs of a long text.
struct Parts {
    words: Vec<Range<usize>>,
    sentences: Vec<Range<usize>>,
    paragraphs: Vec<Range<usize>>,
}

/// About 50k words in 500 paragraphs, with headings and bold Markdown.
fn long_text() -> (String, Parts) {
    let mut text = String::from("Long title\n\n");
    let mut words: Vec<Range<usize>> = Vec::new();
    let mut sentences: Vec<Range<usize>> = Vec::new();
    let mut paragraphs: Vec<Range<usize>> = Vec::new();
    for paragraph in 0..PARAGRAPHS {
        if paragraph % 25 == 0 {
            text.push_str("## Part heading\n\n");
        }
        let paragraph_start = text.len();
        let mut sentence_start = text.len();
        for word in 0..WORDS_PER_PARAGRAPH {
            let start = text.len();
            let name = WORDS[(paragraph * 7 + word) % WORDS.len()];
            if word % 30 == 3 {
                text.push_str("**");
                text.push_str(name);
                text.push_str("**");
            } else {
                text.push_str(name);
            }
            words.push(start..text.len());
            let ends_sentence = word % 10 == 9;
            let last = word + 1 == WORDS_PER_PARAGRAPH;
            if ends_sentence {
                text.push('.');
                sentences.push(sentence_start..text.len());
            }
            if !last {
                text.push(' ');
            }
            if ends_sentence && !last {
                sentence_start = text.len();
            }
        }
        paragraphs.push(paragraph_start..text.len());
        text.push_str("\n\n");
    }
    (
        text,
        Parts {
            words,
            sentences,
            paragraphs,
        },
    )
}

/// The long text with 200 Slots (Word, Sentence, Paragraph, and Word Slots nested in
/// Paragraph Slots), 50 Ghost ranges, and 100 Lab marks.
pub fn fixture() -> Fixture {
    let (text, parts) = long_text();
    let Parts {
        words,
        sentences,
        paragraphs,
    } = parts;
    let mut document = Document::new(&text);
    let mut word_slots = Vec::new();
    for paragraph in (0..PARAGRAPHS).step_by(17).take(30) {
        let range = paragraphs[paragraph].clone();
        document
            .create_slot(
                range,
                Level::Paragraph,
                "A new paragraph.",
                Provenance::Writer,
            )
            .expect("paragraph Slot");
        let nested = words[paragraph * WORDS_PER_PARAGRAPH + 5].clone();
        word_slots.push(
            document
                .create_slot(nested, Level::Word, "nested", Provenance::Ai)
                .expect("nested Slot"),
        );
    }
    let free = |range: &Range<usize>| {
        paragraphs
            .iter()
            .step_by(17)
            .take(30)
            .all(|taken| range.end <= taken.start || taken.end <= range.start)
    };
    let mut free_words = words.iter().filter(|range| free(range)).step_by(97);
    for _ in 0..100 {
        let range = free_words.next().expect("free word").clone();
        word_slots.push(
            document
                .create_slot(range, Level::Word, "other", Provenance::Writer)
                .expect("Word Slot"),
        );
    }
    let mut free_sentences = sentences
        .iter()
        .filter(|range| free(range))
        .skip(3)
        .step_by(41);
    for _ in 0..50 {
        let range = free_sentences.next().expect("free sentence").clone();
        document
            .create_slot(range, Level::Sentence, "Another sentence.", Provenance::Ai)
            .ok();
    }
    let mut free_ghosts = words
        .iter()
        .filter(|range| free(range))
        .skip(50)
        .step_by(331);
    for _ in 0..50 {
        let range = free_ghosts.next().expect("free ghost").clone();
        if document.slot_over(&range).is_none() {
            document.ghost(range).expect("Ghost range");
        }
    }
    word_slots.push(
        document
            .create_slot(0..4, Level::Word, "Short", Provenance::Writer)
            .expect("title Slot"),
    );
    let request = document.lab_request(LabCheck::PunctuationTypos);
    let findings: Vec<LabFinding> = words
        .iter()
        .skip(11)
        .step_by(487)
        .take(100)
        .map(|range| LabFinding {
            range: range.clone(),
            note: Some("check this".to_owned()),
            fix: Some("fixed".to_owned()),
        })
        .collect();
    document
        .apply_lab_result(&request, &findings)
        .expect("Lab marks");
    Fixture {
        document,
        word_slots,
    }
}
