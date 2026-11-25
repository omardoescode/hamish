use std::ops::Range;
use std::sync::Arc;

use app::{Ai, AiFailure, LabRow, LabStatus, RunLabCheck, Workspace};
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, Point, TestAppContext, VisualTestContext, point,
    px,
};
use hamish_core::{
    AiAdapter, AiError, AiRequest, CancelToken, Document, LabCheck, LabFinding, LabRequest,
    LoadedSettings, Settings, TrimRequest,
};

/// Gives fixed findings for each Lab check, or fails, as an AI backend would.
struct FakeAdapter(Result<Vec<(LabCheck, LabFinding)>, AiError>);

impl AiAdapter for FakeAdapter {
    fn variants(
        &self,
        _request: &AiRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<String>, AiError> {
        Ok(Vec::new())
    }

    fn lab_check(
        &self,
        request: &LabRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<LabFinding>, AiError> {
        let findings = self.0.clone()?;
        Ok(findings
            .into_iter()
            .filter(|(check, _)| *check == request.check())
            .map(|(_, finding)| finding)
            .collect())
    }

    fn trim(
        &self,
        _request: &TrimRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<Range<usize>>, AiError> {
        Ok(Vec::new())
    }
}

fn finding(check: LabCheck, range: Range<usize>, fix: Option<&str>) -> (LabCheck, LabFinding) {
    (
        check,
        LabFinding {
            range,
            note: Some("why".to_owned()),
            fix: fix.map(str::to_owned),
        },
    )
}

/// "Title\nI saw teh cat. It was very good." ("teh" is 12..15, "very" is 26..30).
const TEXT: &str = "Title\nI saw teh cat. It was very good.";

/// Opens a window on `TEXT` with `ai` (or no AI backend) and shows the Chrome.
fn open(
    cx: &mut TestAppContext,
    ai: Option<FakeAdapter>,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    if let Some(ai) = ai {
        cx.update(|cx| cx.set_global(Ai(Arc::new(ai))));
    }
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new(TEXT), window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn typo_and_hedge() -> FakeAdapter {
    FakeAdapter(Ok(vec![
        finding(LabCheck::PunctuationTypos, 12..15, Some("the")),
        finding(LabCheck::HedgesFiller, 26..30, None),
    ]))
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).expect("the element is on screen");
    cx.simulate_click(bounds.center(), Modifiers::none());
}

fn lab_open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).lab().read(cx).is_open())
}

fn rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<LabRow> {
    cx.read(|cx| workspace.read(cx).lab().read(cx).rows(cx))
}

fn status(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, check: LabCheck) -> LabStatus {
    rows(workspace, cx)
        .into_iter()
        .find(|row| row.check == check)
        .map_or(LabStatus::Idle, |row| row.status)
}

#[gpui::test]
fn undo_and_redo_work_while_the_lab_popover_has_focus(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(typo_and_hedge()));
    cx.simulate_input("A ");
    click(cx, "lab-button");

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&workspace, cx), TEXT);
    assert!(lab_open(&workspace, cx));

    cx.simulate_keystrokes("ctrl-shift-z");
    assert_eq!(text(&workspace, cx), format!("A {TEXT}"));
}

#[gpui::test]
fn the_lab_button_opens_and_closes_the_popover_and_escape_closes_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(typo_and_hedge()));
    assert!(!lab_open(&workspace, cx));

    click(cx, "lab-button");
    assert!(lab_open(&workspace, cx));
    let labels: Vec<&str> = rows(&workspace, cx).iter().map(|row| row.label).collect();
    assert_eq!(
        labels,
        vec![
            "Fix punctuation and typos",
            "Mark the weakest sentences",
            "Mark sentences that run long",
            "Mark convoluted sentences",
            "Mark words that don't fit the tone",
            "Mark hedges and filler",
        ]
    );

    click(cx, "lab-button");
    assert!(!lab_open(&workspace, cx));

    click(cx, "lab-button");
    cx.simulate_keystrokes("escape");
    assert!(!lab_open(&workspace, cx));
}

fn text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .text()
            .to_owned()
    })
}

fn mark_ranges(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Range<usize>> {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .lab_marks()
            .iter()
            .map(|mark| mark.range.clone())
            .collect()
    })
}

#[gpui::test]
fn a_check_shows_progress_then_marks_the_text_and_keeps_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(typo_and_hedge()));
    click(cx, "lab-button");

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(RunLabCheck(LabCheck::HedgesFiller)), cx);
    });
    assert_eq!(
        status(&workspace, cx, LabCheck::HedgesFiller),
        LabStatus::Running
    );
    assert_eq!(mark_ranges(&workspace, cx), Vec::<Range<usize>>::new());

    cx.run_until_parked();
    assert_eq!(
        status(&workspace, cx, LabCheck::HedgesFiller),
        LabStatus::Marked(1)
    );
    assert_eq!(mark_ranges(&workspace, cx), vec![26..30]);
    assert_eq!(text(&workspace, cx), TEXT);
    assert_eq!(
        status(&workspace, cx, LabCheck::PunctuationTypos),
        LabStatus::Idle
    );
}

#[expect(clippy::expect_used, reason = "test fixture")]
/// A window point in the middle of the row of byte `offset`, just right of it.
fn point_at(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    offset: usize,
) -> Point<gpui::Pixels> {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    let bounds = cx
        .update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.bounds_for_range(offset..offset, Bounds::default(), window, cx)
            })
        })
        .expect("the editor painted the text");
    bounds.origin + point(px(1.), bounds.size.height / 2.)
}

fn click_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, offset: usize) {
    let position = point_at(workspace, cx, offset);
    cx.simulate_click(position, Modifiers::none());
}

/// Runs the checks of rows `rows` from the popover, then closes it.
fn run_checks(cx: &mut VisualTestContext, rows: &[&'static str]) {
    click(cx, "lab-button");
    for row in rows {
        click(cx, row);
    }
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
}

fn card(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Range<usize>> {
    cx.read(|cx| {
        workspace
            .read(cx)
            .lab()
            .read(cx)
            .mark_at_caret(cx)
            .map(|mark| mark.range)
    })
}

#[gpui::test]
fn accept_on_the_mark_card_puts_the_fix_in_place_and_undo_brings_the_mark_back(
    cx: &mut TestAppContext,
) {
    let (workspace, cx) = open(cx, Some(typo_and_hedge()));
    run_checks(cx, &["lab-check-0"]);
    assert_eq!(
        status(&workspace, cx, LabCheck::PunctuationTypos),
        LabStatus::Marked(1)
    );

    click_text(&workspace, cx, 13);
    assert_eq!(card(&workspace, cx), Some(12..15));
    click(cx, "lab-mark-accept");

    assert_eq!(
        text(&workspace, cx),
        "Title\nI saw the cat. It was very good."
    );
    assert_eq!(mark_ranges(&workspace, cx), Vec::<Range<usize>>::new());
    assert_eq!(card(&workspace, cx), None);

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&workspace, cx), TEXT);
    assert_eq!(mark_ranges(&workspace, cx), vec![12..15]);
}

#[gpui::test]
fn dismiss_on_the_mark_card_removes_the_mark_and_keeps_the_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(typo_and_hedge()));
    run_checks(cx, &["lab-check-0", "lab-check-5"]);
    assert_eq!(mark_ranges(&workspace, cx), vec![12..15, 26..30]);

    click_text(&workspace, cx, 27);
    assert_eq!(card(&workspace, cx), Some(26..30));
    assert_eq!(cx.debug_bounds("lab-mark-accept"), None);
    click(cx, "lab-mark-dismiss");

    assert_eq!(text(&workspace, cx), TEXT);
    assert_eq!(mark_ranges(&workspace, cx), vec![12..15]);
    assert_eq!(
        status(&workspace, cx, LabCheck::HedgesFiller),
        LabStatus::Marked(0)
    );
}

/// The clicked mark at the caret is the selected one, and its card sits just under it.
#[expect(clippy::expect_used, reason = "test fixture")]
fn assert_the_card_sits_under_the_selected_mark(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    range: Range<usize>,
) {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    let selected = cx.read(|cx| {
        editor
            .read(cx)
            .selected_lab_mark()
            .map(|mark| mark.range.clone())
    });
    assert_eq!(selected, Some(range));
    let mark = cx
        .read(|cx| editor.read(cx).selected_lab_mark_bounds())
        .expect("the editor painted the selected mark");
    let card = bounds(cx, "lab-mark-card");
    assert!(
        mark.bottom() <= card.top() && card.top() <= mark.bottom() + px(12.),
        "the card {card:?} is not just under the mark {mark:?}"
    );
    assert!(
        (card.left() - mark.left()).abs() <= px(1.),
        "the card {card:?} does not start at the mark {mark:?}"
    );
}

#[gpui::test]
fn a_clicked_mark_is_selected_and_its_card_shows_under_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(typo_and_hedge()));
    run_checks(cx, &["lab-check-0", "lab-check-5"]);
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    assert!(cx.read(|cx| editor.read(cx).selected_lab_mark().is_none()));

    click_text(&workspace, cx, 27);

    assert_the_card_sits_under_the_selected_mark(&workspace, cx, 26..30);
}

#[gpui::test]
fn a_clicked_mark_shows_its_card_while_the_lab_popover_is_open(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(typo_and_hedge()));
    run_checks(cx, &["lab-check-0"]);
    click(cx, "lab-button");

    click_text(&workspace, cx, 13);

    assert!(lab_open(&workspace, cx));
    assert!(cx.debug_bounds("lab-popover").is_some());
    assert_the_card_sits_under_the_selected_mark(&workspace, cx, 12..15);
    click(cx, "lab-mark-accept");
    assert_eq!(
        text(&workspace, cx),
        "Title\nI saw the cat. It was very good."
    );
}

#[gpui::test]
fn an_ai_failure_shows_in_the_row_and_changes_nothing(cx: &mut TestAppContext) {
    let failing = FakeAdapter(Err(AiError::Declined("no".to_owned())));
    let (workspace, cx) = open(cx, Some(failing));
    click(cx, "lab-button");

    click(cx, "lab-check-3");

    assert_eq!(
        status(&workspace, cx, LabCheck::ConvolutedSentences),
        LabStatus::Failed(AiFailure::Ai(AiError::Declined("no".to_owned())))
    );
    assert_eq!(mark_ranges(&workspace, cx), Vec::<Range<usize>>::new());
    assert_eq!(text(&workspace, cx), TEXT);
}

#[gpui::test]
fn a_click_on_the_header_shows_what_each_check_does(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(typo_and_hedge()));
    click(cx, "lab-button");
    assert!(rows(&workspace, cx).iter().all(|row| row.guide.is_none()));

    click(cx, "lab-guide");

    assert_eq!(
        rows(&workspace, cx).first().and_then(|row| row.guide),
        Some("Proposes small fixes; you accept or dismiss each.")
    );
    assert!(rows(&workspace, cx).iter().all(|row| row.guide.is_some()));
}

const TRIM_LEVEL_BUTTONS: [&str; 5] = [
    "trim-level-0",
    "trim-level-1",
    "trim-level-2",
    "trim-level-3",
    "trim-level-4",
];

#[gpui::test]
fn the_trim_level_buttons_fit_the_popover_and_do_not_overlap(cx: &mut TestAppContext) {
    let (_, cx) = open(cx, None);
    click(cx, "lab-button");

    let popover = cx.debug_bounds("lab-popover").expect("the popover is open");
    let buttons: Vec<_> = TRIM_LEVEL_BUTTONS
        .into_iter()
        .map(|selector| {
            cx.debug_bounds(selector)
                .expect("the popover shows the button")
        })
        .collect();
    for (index, button) in buttons.iter().enumerate() {
        assert!(
            popover.left() <= button.left() && button.right() <= popover.right(),
            "trim-level-{index} {button:?} is outside the popover {popover:?}"
        );
        for (other_index, other) in buttons.iter().enumerate().skip(index + 1) {
            assert!(
                !button.intersects(other),
                "trim-level-{index} {button:?} overlaps trim-level-{other_index} {other:?}"
            );
        }
    }
}

fn open_scaled(
    cx: &mut TestAppContext,
    ui_scale: f32,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    cx.update(|cx| {
        app::use_settings(
            None,
            LoadedSettings {
                settings: Settings {
                    ui_scale,
                    ..Settings::default()
                },
                problem: None,
            },
            cx,
        );
        cx.set_global(Ai(Arc::new(typo_and_hedge())));
    });
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new(TEXT), window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<gpui::Pixels> {
    cx.debug_bounds(selector)
        .expect("the window shows the element")
}

/// Runs "Mark hedges and filler" to its end, then starts "Mark words that don't fit the
/// tone" and checks its progress at `ui_scale` while the AI works.
fn assert_a_running_check_shows_the_ai_bar_clear_of_the_rows(
    cx: &mut TestAppContext,
    ui_scale: f32,
) {
    let (workspace, cx) = open_scaled(cx, ui_scale);
    click(cx, "lab-button");
    click(cx, "lab-check-5");
    assert_eq!(cx.debug_bounds("ai-bar"), None);

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(RunLabCheck(LabCheck::OffToneWords)), cx);
    });
    assert_eq!(
        status(&workspace, cx, LabCheck::OffToneWords),
        LabStatus::Running
    );

    let bar = bounds(cx, "ai-bar");
    let popover = bounds(cx, "lab-popover");
    assert!(
        bar.bottom() <= popover.top(),
        "the AI bar {bar:?} overlaps the popover {popover:?}"
    );
    let label = bounds(cx, "lab-check-label-4");
    assert!(
        popover.left() <= label.left() && label.right() <= popover.right(),
        "the running label {label:?} is outside the popover {popover:?}"
    );
    let marked_label = bounds(cx, "lab-check-label-5");
    let marked_status = bounds(cx, "lab-check-status-5");
    assert!(
        !marked_label.intersects(&marked_status),
        "the label {marked_label:?} overlaps its status {marked_status:?}"
    );
    assert!(
        marked_status.right() <= popover.right(),
        "the status {marked_status:?} is outside the popover {popover:?}"
    );

    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("ai-bar"), None);
}

#[gpui::test]
fn a_running_check_shows_the_ai_bar_clear_of_the_rows(cx: &mut TestAppContext) {
    assert_a_running_check_shows_the_ai_bar_clear_of_the_rows(cx, 1.);
}

#[gpui::test]
fn a_running_check_shows_the_ai_bar_clear_of_the_rows_at_a_large_ui_scale(cx: &mut TestAppContext) {
    assert_a_running_check_shows_the_ai_bar_clear_of_the_rows(cx, 2.);
}
