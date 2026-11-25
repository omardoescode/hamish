use std::ops::Range;

use ka_teb_core::{
    AiAdapter, AiError, AiRequest, Document, LabCheck, LabError, LabFinding, LabMark, LabMarkId,
    LabRequest,
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
