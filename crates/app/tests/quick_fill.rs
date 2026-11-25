use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use app::{Ai, AiAlternativesError, AiFailure, AiStatus, AlternativesPanel, Workspace};
use gpui::{Entity, Keystroke, TestAppContext, VisualTestContext};
use hamish_core::{
    AiAdapter, AiError, AiRequest, AiTrigger, CancelToken, Document, LabFinding, LabRequest, Level,
    LoadedSettings, Provenance, Settings, TrimRequest,
};

/// Gives fixed Variant texts, or fails, as an AI backend would, and keeps each request.
struct FakeAdapter {
    answer: Result<Vec<&'static str>, AiError>,
    requests: Arc<Mutex<Vec<AiRequest>>>,
}

impl AiAdapter for FakeAdapter {
    fn variants(&self, request: &AiRequest, _cancel: &CancelToken) -> Result<Vec<String>, AiError> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        let texts = self.answer.clone()?;
        Ok(texts.into_iter().map(str::to_owned).collect())
    }

    fn lab_check(
        &self,
        _request: &LabRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<LabFinding>, AiError> {
        Ok(Vec::new())
    }

    fn trim(
        &self,
        _request: &TrimRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<Range<usize>>, AiError> {
        Ok(Vec::new())
    }
}

/// Keeps the cancel token of each call and gives no Variant.
struct CancelWatch(Arc<Mutex<Vec<CancelToken>>>);

impl AiAdapter for CancelWatch {
    fn variants(&self, _request: &AiRequest, cancel: &CancelToken) -> Result<Vec<String>, AiError> {
        if let Ok(mut cancels) = self.0.lock() {
            cancels.push(cancel.clone());
        }
        Ok(Vec::new())
    }

    fn lab_check(
        &self,
        _request: &LabRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<LabFinding>, AiError> {
        Ok(Vec::new())
    }

    fn trim(
        &self,
        _request: &TrimRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<Range<usize>>, AiError> {
        Ok(Vec::new())
    }
}

/// The five results of the demo for "dominates" .
const DEMO: [&str; 5] = [
    "rules",
    "wins",
    "takes over",
    "commands attention",
    "reigns",
];

/// Time between two result batches.
const BATCH: Duration = Duration::from_millis(150);

/// "Title\nShe dominates the room." with a Word Slot on "dominates": "dominates", "owns"
/// (active).
#[expect(clippy::expect_used, reason = "test fixture")]
fn dominates_document() -> Document {
    let mut document = Document::new("Title\nShe dominates the room.");
    let slot = document
        .create_slot(10..19, Level::Word, "owns", Provenance::Writer)
        .expect("valid range");
    document.set_active(slot, 1).expect("the Variant exists");
    document
}

/// Opens the panel on the "owns" Slot.
fn open<'a>(
    cx: &'a mut TestAppContext,
    answer: Result<Vec<&'static str>, AiError>,
) -> (
    Entity<Workspace>,
    &'a mut VisualTestContext,
    Arc<Mutex<Vec<AiRequest>>>,
) {
    open_with(cx, answer, Settings::default())
}

fn open_with<'a>(
    cx: &'a mut TestAppContext,
    answer: Result<Vec<&'static str>, AiError>,
    settings: Settings,
) -> (
    Entity<Workspace>,
    &'a mut VisualTestContext,
    Arc<Mutex<Vec<AiRequest>>>,
) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let loaded = LoadedSettings {
        settings,
        problem: None,
    };
    cx.update(|cx| app::use_settings(None, loaded, cx));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let ai = FakeAdapter {
        answer,
        requests: Arc::clone(&requests),
    };
    cx.update(|cx| cx.set_global(Ai(Arc::new(ai))));
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(dominates_document(), window, cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("down right right right right right ctrl-shift-a");
    (workspace, cx, requests)
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn panel(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<AlternativesPanel> {
    cx.read(|cx| workspace.read(cx).alternatives_panel().cloned())
        .expect("the panel is open")
}

fn rows(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Vec<(String, Provenance, bool)> {
    let panel = panel(workspace, cx);
    cx.read(|cx| {
        panel
            .read(cx)
            .rows(cx)
            .into_iter()
            .map(|row| (row.text, row.provenance, row.active))
            .collect()
    })
}

fn row(text: &str, provenance: Provenance, active: bool) -> (String, Provenance, bool) {
    (text.to_owned(), provenance, active)
}

fn status(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> AiStatus<AiAlternativesError> {
    let panel = panel(workspace, cx);
    cx.read(|cx| panel.read(cx).ai_status())
}

fn shown_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let editor = workspace.read(cx).editor().read(cx);
        editor.shown_document().text().to_owned()
    })
}

/// Types `key` without waiting for the AI.
#[expect(clippy::expect_used, reason = "test fixture")]
fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).expect("a valid keystroke");
    cx.update(|window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
}

fn the_two_writer_rows() -> Vec<(String, Provenance, bool)> {
    vec![
        row("dominates", Provenance::Writer, false),
        row("owns", Provenance::Writer, true),
    ]
}

#[gpui::test]
fn typing_two_question_marks_starts_a_quick_fill_that_never_becomes_a_variant(
    cx: &mut TestAppContext,
) {
    let (workspace, cx, requests) = open(cx, Ok(DEMO.to_vec()));

    press(cx, "?");
    assert_eq!(shown_text(&workspace, cx), "Title\nShe ? the room.");
    press(cx, "?");

    assert_eq!(status(&workspace, cx), AiStatus::Running);
    assert_eq!(shown_text(&workspace, cx), "Title\nShe owns the room.");
    assert_eq!(rows(&workspace, cx), the_two_writer_rows());
    press(cx, "enter");
    assert_eq!(rows(&workspace, cx), the_two_writer_rows());

    cx.run_until_parked();
    let sent = requests.lock().map(|requests| {
        requests
            .iter()
            .map(|request| (request.trigger(), request.variants().to_vec()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        sent.ok(),
        Some(vec![(
            AiTrigger::QuickFill,
            vec!["dominates".to_owned(), "owns".to_owned()]
        )])
    );
}

#[gpui::test]
fn quick_fill_results_come_in_batches_under_the_variants_and_the_active_one_stays(
    cx: &mut TestAppContext,
) {
    let (workspace, cx, _) = open(cx, Ok(DEMO.to_vec()));

    cx.simulate_input("??");
    assert_eq!(rows(&workspace, cx).len(), 4);
    assert_eq!(status(&workspace, cx), AiStatus::Running);
    cx.executor().advance_clock(BATCH);
    assert_eq!(rows(&workspace, cx).len(), 6);
    cx.executor().advance_clock(BATCH);

    assert_eq!(status(&workspace, cx), AiStatus::Done);
    assert_eq!(
        rows(&workspace, cx),
        vec![
            row("dominates", Provenance::Writer, false),
            row("owns", Provenance::Writer, true),
            row("rules", Provenance::Ai, false),
            row("wins", Provenance::Ai, false),
            row("takes over", Provenance::Ai, false),
            row("commands attention", Provenance::Ai, false),
            row("reigns", Provenance::Ai, false),
        ]
    );
    assert_eq!(shown_text(&workspace, cx), "Title\nShe owns the room.");
}

#[gpui::test]
fn after_a_quick_fill_the_empty_row_is_gone_until_down_past_the_last_row(cx: &mut TestAppContext) {
    let (_workspace, cx, _) = open(cx, Ok(DEMO.to_vec()));
    cx.simulate_input("??");
    cx.executor().advance_clock(BATCH * 3);
    cx.update(|window, _| window.refresh());
    assert!(cx.debug_bounds("alternatives-row-7").is_none());
    assert!(cx.debug_bounds("alternatives-row-1").is_some());

    cx.simulate_keystrokes("down down down down down down");
    cx.update(|window, _| window.refresh());

    assert!(cx.debug_bounds("alternatives-row-7").is_some());
}

#[gpui::test]
fn the_quick_fill_row_dances_while_the_ai_works(cx: &mut TestAppContext) {
    let (_workspace, cx, _) = open(cx, Ok(DEMO.to_vec()));

    press(cx, "?");
    press(cx, "?");
    cx.update(|window, _| window.refresh());

    assert!(cx.debug_bounds("alternatives-loading").is_some());
    assert!(cx.debug_bounds("alternatives-loading-still").is_none());
}

#[gpui::test]
fn with_reduced_motion_the_quick_fill_row_shows_a_still_question_mark_pair(
    cx: &mut TestAppContext,
) {
    let settings = Settings {
        reduced_motion: true,
        ..Settings::default()
    };
    let (_workspace, cx, _) = open_with(cx, Ok(DEMO.to_vec()), settings);

    press(cx, "?");
    press(cx, "?");
    cx.update(|window, _| window.refresh());

    assert!(cx.debug_bounds("alternatives-loading-still").is_some());
}

#[gpui::test]
fn escape_while_the_row_dances_cancels_the_quick_fill_and_keeps_the_panel_open(
    cx: &mut TestAppContext,
) {
    let (workspace, cx, _) = open(cx, Ok(DEMO.to_vec()));
    press(cx, "?");
    press(cx, "?");

    press(cx, "escape");
    cx.run_until_parked();
    cx.executor().advance_clock(BATCH * 3);

    assert_eq!(status(&workspace, cx), AiStatus::Idle);
    assert_eq!(rows(&workspace, cx), the_two_writer_rows());
    cx.simulate_input("x");
    assert_eq!(shown_text(&workspace, cx), "Title\nShe x the room.");
}

#[gpui::test]
fn a_failed_quick_fill_shows_why_and_the_row_is_empty_again(cx: &mut TestAppContext) {
    let (workspace, cx, _) = open(cx, Err(AiError::Declined("no".to_owned())));

    cx.simulate_input("??");

    assert_eq!(
        status(&workspace, cx),
        AiStatus::Failed(AiFailure::Ai(AiError::Declined("no".to_owned())))
    );
    assert_eq!(rows(&workspace, cx), the_two_writer_rows());
    cx.simulate_input("x");
    assert_eq!(shown_text(&workspace, cx), "Title\nShe x the room.");
}

#[gpui::test]
fn quick_fill_works_on_text_with_no_slot_yet_at_sentence_level(cx: &mut TestAppContext) {
    let (workspace, cx, requests) = open(cx, Ok(vec!["She rules the room."]));
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("ctrl-shift-a");
    let tab = cx
        .debug_bounds("alternatives-tab-sentence")
        .map(|bounds| bounds.center());
    if let Some(tab) = tab {
        cx.simulate_click(tab, gpui::Modifiers::none());
    }

    cx.simulate_input("??");
    cx.executor().advance_clock(BATCH * 3);

    assert_eq!(
        rows(&workspace, cx),
        vec![
            row("She owns the room.", Provenance::Writer, true),
            row("She rules the room.", Provenance::Ai, false),
        ]
    );
    let level = requests
        .lock()
        .ok()
        .and_then(|requests| requests.first().map(AiRequest::level));
    assert_eq!(level, Some(Level::Sentence));
}

#[gpui::test]
fn escape_while_the_row_dances_cancels_the_ai_call(cx: &mut TestAppContext) {
    let (_workspace, cx, _) = open(cx, Ok(DEMO.to_vec()));
    let cancels = Arc::new(Mutex::new(Vec::new()));
    let watch = CancelWatch(Arc::clone(&cancels));
    cx.update(|_, cx| cx.set_global(Ai(Arc::new(watch))));
    press(cx, "?");
    press(cx, "?");
    let called = || cancels.lock().map_or(0, |cancels| cancels.len()) > 0;
    while !called() && cx.executor().tick() {}
    let token = cancels
        .lock()
        .ok()
        .and_then(|cancels| cancels.first().cloned());
    let Some(token) = token else {
        panic!("the AI was called");
    };
    assert!(!token.is_cancelled());

    press(cx, "escape");

    assert!(token.is_cancelled());
}
