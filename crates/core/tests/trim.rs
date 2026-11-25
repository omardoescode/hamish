use std::ops::Range;

use ka_teb_core::{
    AiAdapter, AiError, AiRequest, Document, LabFinding, LabRequest, Level, Provenance, TrimCut,
    TrimError, TrimLevel, TrimRequest,
};

/// Returns fixed cuts, as an AI backend would.
struct FakeAdapter(Vec<Range<usize>>);

impl AiAdapter for FakeAdapter {
    fn variants(&self, _request: &AiRequest) -> Result<Vec<String>, AiError> {
        Ok(Vec::new())
    }

    fn lab_check(&self, _request: &LabRequest) -> Result<Vec<LabFinding>, AiError> {
        Ok(Vec::new())
    }

    fn trim(&self, _request: &TrimRequest) -> Result<Vec<Range<usize>>, AiError> {
        Ok(self.0.clone())
    }
}

/// 107 sentences of 5 words: 535 words. Sentence `i` with its space is bytes `19 * i..19 * (i + 1)`.
fn draft() -> String {
    vec!["The cat sat on it."; 107].join(" ")
}

/// The first `count` sentences of [`draft`], 5 words each.
fn sentences(count: usize) -> Vec<Range<usize>> {
    (0..count).map(|i| 19 * i..19 * (i + 1)).collect()
}

/// Runs a Trim at `level` with the fake adapter giving `cuts`.
fn run(
    document: &mut Document,
    level: TrimLevel,
    cuts: Vec<Range<usize>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = document.trim_request(level).ok_or("no request")?;
    let cuts = FakeAdapter(cuts).trim(&request)?;
    Ok(document.apply_trim_result(&request, &cuts)?)
}

fn open(ranges: &[Range<usize>]) -> Vec<TrimCut> {
    ranges
        .iter()
        .map(|range| TrimCut {
            range: range.clone(),
            kept: false,
        })
        .collect()
}

#[test]
fn the_trim_levels_are_the_spec_levels_with_their_targets() {
    let targets: Vec<_> = TrimLevel::ALL
        .iter()
        .map(|level| (*level, level.target_percent()))
        .collect();

    assert_eq!(
        targets,
        [
            (TrimLevel::Original, 0),
            (TrimLevel::SlightTrim, 10),
            (TrimLevel::TightenMore, 20),
            (TrimLevel::EvenSharper, 30),
            (TrimLevel::CutInHalf, 50),
        ]
    );
}

#[test]
fn a_trim_result_proposes_cuts_with_word_counts_and_keeps_the_text() {
    let text = draft();
    let mut document = Document::new(&text);
    let request = document.trim_request(TrimLevel::SlightTrim).unwrap();
    assert_eq!(request.level(), TrimLevel::SlightTrim);
    assert_eq!(request.text(), text);
    let cuts = FakeAdapter(sentences(11)).trim(&request).unwrap();

    document.apply_trim_result(&request, &cuts).unwrap();

    let proposal = document.trim_proposal().unwrap();
    assert_eq!(proposal.to_string(), "535 → 480 words · -10%");
    assert_eq!(proposal.level(), TrimLevel::SlightTrim);
    assert_eq!(proposal.words_before(), 535);
    assert_eq!(proposal.words_after(), 480);
    assert_eq!(proposal.percent(), -10);
    assert_eq!(proposal.cuts(), open(&sentences(11)));
    assert_eq!(document.text(), text);
    assert_eq!(document.ghost_ranges(), []);
    assert!(!document.undo());
}

#[test]
fn a_new_level_replaces_the_proposal() {
    let mut document = Document::new(&draft());
    run(&mut document, TrimLevel::SlightTrim, sentences(11)).unwrap();

    run(&mut document, TrimLevel::CutInHalf, sentences(54)).unwrap();

    let proposal = document.trim_proposal().unwrap();
    assert_eq!(proposal.level(), TrimLevel::CutInHalf);
    assert_eq!(proposal.to_string(), "535 → 265 words · -50%");
    assert_eq!(proposal.cuts().len(), 54);
}

#[test]
fn done_and_the_original_level_close_the_proposal_and_keep_the_text() {
    let text = draft();
    let mut document = Document::new(&text);
    run(&mut document, TrimLevel::SlightTrim, sentences(11)).unwrap();

    document.close_trim();

    assert_eq!(document.trim_proposal(), None);
    assert_eq!(document.text(), text);
    assert!(!document.undo());
}

#[test]
fn cuts_with_an_empty_or_invalid_range_are_dropped_and_overlapping_cuts_join() {
    let mut document = Document::new("Το γάλα ήταν πολύ πολύ κρύο.");
    run(
        &mut document,
        TrimLevel::SlightTrim,
        vec![41..41, 1..3, 2..90, 31..40, 22..32, 0..4],
    )
    .unwrap();

    assert_eq!(
        document.trim_proposal().unwrap().cuts(),
        open(&[0..4, 22..40])
    );
    assert_eq!(
        document.trim_proposal().unwrap().to_string(),
        "6 → 3 words · -50%"
    );
}

#[test]
fn a_trim_result_for_text_that_changed_since_the_request_is_refused() {
    let mut document = Document::new(&draft());
    let request = document.trim_request(TrimLevel::SlightTrim).unwrap();
    document.insert(0, "Well, ").unwrap();

    assert_eq!(
        document.apply_trim_result(&request, &sentences(1)),
        Err(TrimError::Stale)
    );
    assert_eq!(document.trim_proposal(), None);
}

#[test]
fn keep_takes_one_cut_out_of_the_proposal_and_updates_the_counts() {
    let text = draft();
    let mut document = Document::new(&text);
    run(&mut document, TrimLevel::SlightTrim, sentences(11)).unwrap();

    document.keep_trim_cut(3).unwrap();

    let proposal = document.trim_proposal().unwrap();
    assert_eq!(proposal.to_string(), "535 → 485 words · -9%");
    assert_eq!(
        proposal.cuts()[3],
        TrimCut {
            range: 57..76,
            kept: true
        }
    );
    assert_eq!(proposal.cuts().iter().filter(|cut| cut.kept).count(), 1);
    assert_eq!(document.text(), text);
    assert_eq!(document.keep_trim_cut(11), Err(TrimError::NoCut(11)));
}

#[test]
fn make_the_cuts_deletes_the_cuts_still_in_the_proposal_as_one_undo_step() {
    let mut document = Document::new("The cat sat. It was very, very good. The end.");
    run(
        &mut document,
        TrimLevel::TightenMore,
        vec![20..26, 26..31, 37..45],
    )
    .unwrap();
    document.keep_trim_cut(1).unwrap();

    document.make_trim_cuts().unwrap();

    assert_eq!(document.text(), "The cat sat. It was very good. ");
    assert_eq!(document.counter().words, 7);
    assert_eq!(document.trim_proposal(), None);
    assert!(document.undo());
    assert_eq!(
        document.text(),
        "The cat sat. It was very, very good. The end."
    );
    assert!(!document.undo());
    assert!(document.redo());
    assert_eq!(document.text(), "The cat sat. It was very good. ");
}

#[test]
fn make_the_cuts_moves_slots_and_ghost_ranges_around_the_cuts() {
    let mut document = Document::new("The cat sat. It was very, very good. The end.");
    let slot = document
        .create_slot(4..7, Level::Word, "dog", Provenance::Writer)
        .unwrap();
    document.ghost(41..44).unwrap();
    run(&mut document, TrimLevel::TightenMore, vec![13..20, 20..26]).unwrap();

    document.make_trim_cuts().unwrap();

    assert_eq!(document.text(), "The cat sat. very good. The end.");
    assert_eq!(document.slot(slot).unwrap().range, 4..7);
    assert_eq!(document.ghost_ranges(), vec![28..31]);
}

#[test]
fn trim_actions_without_a_proposal_are_refused() {
    let mut document = Document::new("The cat sat.");

    assert_eq!(document.make_trim_cuts(), Err(TrimError::NoProposal));
    assert_eq!(document.keep_trim_cut(0), Err(TrimError::NoProposal));
    assert_eq!(document.text(), "The cat sat.");
}

#[test]
fn the_original_level_sends_no_request() {
    assert_eq!(
        Document::new("The cat sat.").trim_request(TrimLevel::Original),
        None
    );
}
