use std::ops::Range;

use ka_teb_core::{
    AiAdapter, AiError, AiRequest, Document, LabCheck, LabError, LabFinding, LabMark, LabMarkId,
    LabRequest, Level, Provenance, TrimRequest,
};

/// Returns fixed findings, as an AI backend would.
struct FakeAdapter(Vec<LabFinding>);

impl AiAdapter for FakeAdapter {
    fn variants(&self, _request: &AiRequest) -> Result<Vec<String>, AiError> {
        Ok(Vec::new())
    }

    fn lab_check(&self, _request: &LabRequest) -> Result<Vec<LabFinding>, AiError> {
        Ok(self.0.clone())
    }

    fn trim(&self, _request: &TrimRequest) -> Result<Vec<Range<usize>>, AiError> {
        Ok(Vec::new())
    }
}

fn finding(range: Range<usize>, note: Option<&str>, fix: Option<&str>) -> LabFinding {
    LabFinding {
        range,
        note: note.map(str::to_owned),
        fix: fix.map(str::to_owned),
    }
}

/// Runs `check` with the fake adapter giving `findings`.
fn run(
    document: &mut Document,
    check: LabCheck,
    findings: Vec<LabFinding>,
) -> Result<Vec<LabMarkId>, Box<dyn std::error::Error>> {
    let request = document.lab_request(check);
    let findings = FakeAdapter(findings).lab_check(&request)?;
    Ok(document.apply_lab_result(&request, &findings)?)
}

type Shown = (LabCheck, Range<usize>, Option<String>, Option<String>);

fn shown(mark: &LabMark) -> Shown {
    (
        mark.check,
        mark.range.clone(),
        mark.note.clone(),
        mark.fix.clone(),
    )
}

fn marks(document: &Document) -> Vec<Shown> {
    document.lab_marks().iter().map(shown).collect()
}

fn mark(check: LabCheck, range: Range<usize>, note: Option<&str>, fix: Option<&str>) -> Shown {
    (
        check,
        range,
        note.map(str::to_owned),
        fix.map(str::to_owned),
    )
}

const TEXT: &str = "I think the cat sat. It was very good.";

#[test]
fn each_lab_check_marks_the_ranges_the_ai_gives_and_keeps_the_text() {
    for check in LabCheck::ALL {
        let mut document = Document::new(TEXT);
        let request = document.lab_request(check);
        assert_eq!(request.check(), check);
        assert_eq!(request.text(), TEXT);
        let findings = FakeAdapter(vec![
            finding(8..20, Some("why"), None),
            finding(0..7, None, None),
        ])
        .lab_check(&request)
        .unwrap();

        let ids = document.apply_lab_result(&request, &findings).unwrap();

        assert_eq!(document.text(), TEXT);
        assert_eq!(ids.len(), 2);
        assert_eq!(
            marks(&document),
            vec![
                mark(check, 0..7, None, None),
                mark(check, 8..20, Some("why"), None)
            ]
        );
        assert_eq!(
            document
                .lab_marks()
                .iter()
                .map(|mark| mark.id)
                .collect::<Vec<_>>(),
            vec![ids[1], ids[0]]
        );
    }
}

#[test]
fn the_six_lab_checks_are_the_spec_checks() {
    assert_eq!(
        LabCheck::ALL,
        [
            LabCheck::PunctuationTypos,
            LabCheck::WeakestSentences,
            LabCheck::LongSentences,
            LabCheck::ConvolutedSentences,
            LabCheck::OffToneWords,
            LabCheck::HedgesFiller,
        ]
    );
}

#[test]
fn only_punctuation_and_typos_marks_keep_a_proposed_fix() {
    let mut document = Document::new("Teh cat sat");
    run(
        &mut document,
        LabCheck::PunctuationTypos,
        vec![finding(0..3, None, Some("The"))],
    )
    .unwrap();
    run(
        &mut document,
        LabCheck::OffToneWords,
        vec![finding(4..7, None, Some("dog"))],
    )
    .unwrap();

    assert_eq!(
        marks(&document),
        vec![
            mark(LabCheck::PunctuationTypos, 0..3, None, Some("The")),
            mark(LabCheck::OffToneWords, 4..7, None, None),
        ]
    );
    assert_eq!(document.text(), "Teh cat sat");
}

#[test]
fn findings_with_an_empty_or_invalid_range_are_dropped() {
    let mut document = Document::new("Το γάλα.");
    run(
        &mut document,
        LabCheck::WeakestSentences,
        vec![
            finding(3..3, None, None),
            finding(2..40, None, None),
            finding(1..3, None, None),
            finding(0..4, None, None),
        ],
    )
    .unwrap();

    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::WeakestSentences, 0..4, None, None)]
    );
}

#[test]
fn a_new_result_replaces_the_open_marks_of_the_same_check_only() {
    let mut document = Document::new(TEXT);
    run(
        &mut document,
        LabCheck::HedgesFiller,
        vec![finding(0..7, None, None)],
    )
    .unwrap();
    run(
        &mut document,
        LabCheck::LongSentences,
        vec![finding(21..38, None, None)],
    )
    .unwrap();

    run(
        &mut document,
        LabCheck::HedgesFiller,
        vec![finding(28..32, None, None)],
    )
    .unwrap();

    assert_eq!(
        marks(&document),
        vec![
            mark(LabCheck::LongSentences, 21..38, None, None),
            mark(LabCheck::HedgesFiller, 28..32, None, None),
        ]
    );
}

#[test]
fn a_result_for_text_that_changed_since_the_request_is_refused() {
    let mut document = Document::new(TEXT);
    let request = document.lab_request(LabCheck::HedgesFiller);
    document.insert(0, "Well, ").unwrap();

    assert_eq!(
        document.apply_lab_result(&request, &[finding(0..7, None, None)]),
        Err(LabError::Stale)
    );
    assert_eq!(document.lab_marks(), []);
}

#[test]
fn undo_removes_a_lab_result_in_one_step_and_redo_brings_it_back() {
    let mut document = Document::new(TEXT);
    run(
        &mut document,
        LabCheck::HedgesFiller,
        vec![finding(0..7, None, None), finding(28..32, None, None)],
    )
    .unwrap();

    assert!(document.undo());
    assert_eq!(document.lab_marks(), []);
    assert!(!document.undo());
    assert!(document.redo());
    assert_eq!(
        marks(&document),
        vec![
            mark(LabCheck::HedgesFiller, 0..7, None, None),
            mark(LabCheck::HedgesFiller, 28..32, None, None),
        ]
    );
}

#[test]
fn dismiss_removes_the_mark_as_one_undo_step_and_keeps_the_text() {
    let mut document = Document::new(TEXT);
    let ids = run(
        &mut document,
        LabCheck::HedgesFiller,
        vec![finding(0..7, None, None), finding(28..32, None, None)],
    )
    .unwrap();

    document.dismiss_lab_mark(ids[0]).unwrap();

    assert_eq!(document.text(), TEXT);
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::HedgesFiller, 28..32, None, None)]
    );
    assert_eq!(
        document.dismiss_lab_mark(ids[0]),
        Err(LabError::NoMark(ids[0]))
    );
    assert!(document.undo());
    assert_eq!(document.lab_marks().len(), 2);
}

#[test]
fn accept_puts_the_fix_in_place_as_one_undo_step_and_ends_the_mark() {
    let mut document = Document::new("Teh cat sat ,then left.");
    let ids = run(
        &mut document,
        LabCheck::PunctuationTypos,
        vec![
            finding(0..3, None, Some("The")),
            finding(11..13, None, Some(", ")),
        ],
    )
    .unwrap();

    document.accept_lab_fix(ids[1]).unwrap();

    assert_eq!(document.text(), "Teh cat sat, then left.");
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::PunctuationTypos, 0..3, None, Some("The"))]
    );
    assert!(document.undo());
    assert_eq!(document.text(), "Teh cat sat ,then left.");
    assert_eq!(document.lab_marks().len(), 2);
    assert!(document.redo());
    assert_eq!(document.text(), "Teh cat sat, then left.");
}

#[test]
fn accept_of_a_mark_without_a_fix_is_refused_and_changes_nothing() {
    let mut document = Document::new(TEXT);
    let ids = run(
        &mut document,
        LabCheck::HedgesFiller,
        vec![finding(0..7, None, None)],
    )
    .unwrap();

    assert_eq!(
        document.accept_lab_fix(ids[0]),
        Err(LabError::NoFix(ids[0]))
    );
    assert_eq!(document.text(), TEXT);
    assert_eq!(document.lab_marks().len(), 1);
    assert!(document.undo());
    assert!(!document.undo());
}

#[test]
fn marks_move_with_edits_before_them_and_stay_for_edits_after_them() {
    let mut document = Document::new(TEXT);
    run(
        &mut document,
        LabCheck::HedgesFiller,
        vec![finding(2..7, None, None)],
    )
    .unwrap();

    document.insert(0, "So ").unwrap();
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::HedgesFiller, 5..10, None, None)]
    );
    document.insert(5, "I ").unwrap();
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::HedgesFiller, 7..12, None, None)]
    );
    document.insert(12, "!").unwrap();
    document.delete(20..24).unwrap();
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::HedgesFiller, 7..12, None, None)]
    );
    assert_eq!(&document.text()[7..12], "think");
}

#[test]
fn a_mark_stretches_or_shrinks_with_edits_in_its_text() {
    let mut document = Document::new(TEXT);
    run(
        &mut document,
        LabCheck::WeakestSentences,
        vec![finding(21..38, None, None)],
    )
    .unwrap();

    document.replace(28..32, "really very").unwrap();
    assert_eq!(
        document.text(),
        "I think the cat sat. It was really very good."
    );
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::WeakestSentences, 21..45, None, None)]
    );
    document.delete(16..24).unwrap();
    assert_eq!(document.text(), "I think the cat was really very good.");
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::WeakestSentences, 16..37, None, None)]
    );
    document.delete(26..37).unwrap();
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::WeakestSentences, 16..26, None, None)]
    );
}

#[test]
fn a_mark_whose_text_is_deleted_ends() {
    let mut document = Document::new(TEXT);
    run(
        &mut document,
        LabCheck::HedgesFiller,
        vec![finding(28..32, None, None)],
    )
    .unwrap();

    document.delete(27..32).unwrap();

    assert_eq!(document.lab_marks(), []);
    assert!(document.undo());
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::HedgesFiller, 28..32, None, None)]
    );
}

#[test]
fn a_mark_with_a_fix_ends_when_its_text_changes() {
    let mut document = Document::new("Teh cat sat");
    run(
        &mut document,
        LabCheck::PunctuationTypos,
        vec![finding(0..3, None, Some("The"))],
    )
    .unwrap();

    document.insert(3, "e").unwrap();
    assert_eq!(document.lab_marks().len(), 1);
    document.replace(1..3, "h").unwrap();

    assert_eq!(document.text(), "The cat sat");
    assert_eq!(document.lab_marks(), []);
}

#[test]
fn marks_follow_a_variant_switch_like_a_text_edit() {
    let mut document = Document::new("The cat sat. It was very good.");
    run(
        &mut document,
        LabCheck::OffToneWords,
        vec![finding(4..7, None, None), finding(20..24, None, None)],
    )
    .unwrap();
    let slot = document
        .create_slot(4..7, Level::Word, "kitten", Provenance::Writer)
        .unwrap();

    document.set_active(slot, 1).unwrap();

    assert_eq!(document.text(), "The kitten sat. It was very good.");
    assert_eq!(
        marks(&document),
        vec![mark(LabCheck::OffToneWords, 23..27, None, None)]
    );
}
