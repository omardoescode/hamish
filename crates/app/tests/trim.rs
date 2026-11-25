use std::ops::Range;
use std::sync::Arc;

use app::{Ai, RunTrim, TrimBar, Workspace};
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, Point, TestAppContext, VisualTestContext, point,
    px,
};
use ka_teb_core::{
    AiAdapter, AiError, AiRequest, CancelToken, Document, LabFinding, LabRequest, TrimCut,
    TrimLevel, TrimRequest,
};

/// Gives fixed cuts for each Trim level, or fails, as an AI backend would.
struct FakeAdapter(Result<Vec<(TrimLevel, Range<usize>)>, AiError>);

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
        _request: &LabRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<LabFinding>, AiError> {
        Ok(Vec::new())
    }

    fn trim(
        &self,
        request: &TrimRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<Range<usize>>, AiError> {
        let cuts = self.0.clone()?;
        Ok(cuts
            .into_iter()
            .filter(|(level, _)| *level == request.level())
            .map(|(_, range)| range)
            .collect())
    }
}

/// 9 words. "really " is 8..15, "really think " is 8..21, "very " is 33..38.
const TEXT: &str = "Title\nI really think the plan is very good.";

/// Slight trim cuts "really " and "very "; Tighten more cuts "really think " and "very ".
fn two_levels() -> FakeAdapter {
    FakeAdapter(Ok(vec![
        (TrimLevel::SlightTrim, 8..15),
        (TrimLevel::SlightTrim, 33..38),
        (TrimLevel::TightenMore, 8..21),
        (TrimLevel::TightenMore, 33..38),
    ]))
}

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

#[expect(clippy::expect_used, reason = "test fixture")]
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).expect("the element is on screen");
    cx.simulate_click(bounds.center(), Modifiers::none());
}

/// Opens the Lab and clicks Trim level button `index` (0 is Original), then lets the AI answer.
fn trim(cx: &mut VisualTestContext, selector: &'static str) {
    if cx.debug_bounds("trim-level-0").is_none() {
        click(cx, "lab-button");
    }
    click(cx, selector);
    cx.run_until_parked();
}

fn bar(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<TrimBar> {
    cx.read(|cx| workspace.read(cx).lab().read(cx).trim_bar(cx))
}

fn bar_line(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    bar(workspace, cx).map(|bar| bar.line)
}

fn selected(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> TrimLevel {
    cx.read(|cx| workspace.read(cx).lab().read(cx).trim_level(cx))
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

fn cuts(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<TrimCut> {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .trim_proposal()
            .map_or_else(Vec::new, |proposal| proposal.cuts().to_vec())
    })
}

fn cut(range: Range<usize>, kept: bool) -> TrimCut {
    TrimCut { range, kept }
}

#[expect(clippy::expect_used, reason = "test fixture")]
/// A window point just right of the left edge of the glyph at byte `offset`.
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

#[gpui::test]
fn a_trim_level_runs_in_the_background_then_shows_the_bar_and_keeps_the_text(
    cx: &mut TestAppContext,
) {
    let (workspace, cx) = open(cx, Some(two_levels()));
    click(cx, "lab-button");
    assert_eq!(selected(&workspace, cx), TrimLevel::Original);
    assert_eq!(bar_line(&workspace, cx), None);

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(RunTrim(TrimLevel::SlightTrim)), cx);
    });
    assert_eq!(
        bar_line(&workspace, cx),
        Some("Slight trim: trimming…".to_owned())
    );
    assert_eq!(cuts(&workspace, cx), Vec::<TrimCut>::new());

    cx.run_until_parked();
    assert_eq!(
        bar(&workspace, cx),
        Some(TrimBar {
            level: TrimLevel::SlightTrim,
            line: "Slight trim 9 → 7 words · -22%".to_owned(),
            walking: false,
        })
    );
    assert_eq!(selected(&workspace, cx), TrimLevel::SlightTrim);
    assert_eq!(
        cuts(&workspace, cx),
        vec![cut(8..15, false), cut(33..38, false)]
    );
    assert_eq!(text(&workspace, cx), TEXT);
}

#[gpui::test]
fn a_click_on_a_faded_word_keeps_it_and_the_counts_update(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(two_levels()));
    trim(cx, "trim-level-1");

    click_text(&workspace, cx, 10);

    assert_eq!(
        cuts(&workspace, cx),
        vec![cut(8..15, true), cut(33..38, false)]
    );
    assert_eq!(
        bar_line(&workspace, cx),
        Some("Slight trim 9 → 8 words · -11%".to_owned())
    );
    assert_eq!(text(&workspace, cx), TEXT);
}

#[gpui::test]
fn a_click_outside_the_cuts_keeps_nothing(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(two_levels()));
    trim(cx, "trim-level-1");

    click_text(&workspace, cx, 22);

    assert_eq!(
        cuts(&workspace, cx),
        vec![cut(8..15, false), cut(33..38, false)]
    );
}

#[gpui::test]
fn make_the_cuts_deletes_the_open_cuts_as_one_undo_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(two_levels()));
    trim(cx, "trim-level-1");
    click_text(&workspace, cx, 10);

    click(cx, "trim-make-cuts");

    assert_eq!(
        text(&workspace, cx),
        "Title\nI really think the plan is good."
    );
    assert_eq!(bar(&workspace, cx), None);
    assert_eq!(selected(&workspace, cx), TrimLevel::Original);

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&workspace, cx), TEXT);
}

#[gpui::test]
fn walk_through_keeps_or_cuts_each_cut_in_turn(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(two_levels()));
    trim(cx, "trim-level-1");

    click(cx, "trim-walk");
    assert_eq!(bar(&workspace, cx).map(|bar| bar.walking), Some(true));

    click(cx, "trim-walk-keep");
    assert_eq!(text(&workspace, cx), TEXT);
    assert_eq!(
        cuts(&workspace, cx),
        vec![cut(8..15, true), cut(33..38, false)]
    );

    click(cx, "trim-walk-cut");
    assert_eq!(
        text(&workspace, cx),
        "Title\nI really think the plan is good."
    );
    assert_eq!(
        bar(&workspace, cx),
        Some(TrimBar {
            level: TrimLevel::SlightTrim,
            line: "Slight trim 9 → 8 words · -11%".to_owned(),
            walking: false,
        })
    );

    cx.simulate_keystrokes("escape ctrl-z");
    assert_eq!(text(&workspace, cx), TEXT);
}

#[gpui::test]
fn done_closes_the_bar_and_keeps_the_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(two_levels()));
    trim(cx, "trim-level-1");

    click(cx, "trim-done");

    assert_eq!(bar(&workspace, cx), None);
    assert_eq!(cuts(&workspace, cx), Vec::<TrimCut>::new());
    assert_eq!(text(&workspace, cx), TEXT);
}

#[gpui::test]
fn a_new_level_replaces_the_proposal_and_original_clears_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(two_levels()));
    trim(cx, "trim-level-1");

    trim(cx, "trim-level-2");
    assert_eq!(
        bar_line(&workspace, cx),
        Some("Tighten more 9 → 6 words · -33%".to_owned())
    );
    assert_eq!(selected(&workspace, cx), TrimLevel::TightenMore);
    assert_eq!(
        cuts(&workspace, cx),
        vec![cut(8..21, false), cut(33..38, false)]
    );

    trim(cx, "trim-level-0");
    assert_eq!(bar(&workspace, cx), None);
    assert_eq!(selected(&workspace, cx), TrimLevel::Original);
    assert_eq!(cuts(&workspace, cx), Vec::<TrimCut>::new());
    assert_eq!(text(&workspace, cx), TEXT);
}

#[gpui::test]
fn an_ai_failure_shows_in_the_bar_and_changes_nothing(cx: &mut TestAppContext) {
    let failing = FakeAdapter(Err(AiError::Declined("no".to_owned())));
    let (workspace, cx) = open(cx, Some(failing));

    trim(cx, "trim-level-4");

    assert_eq!(
        bar_line(&workspace, cx),
        Some("Cut in half: the AI declined: no".to_owned())
    );
    assert_eq!(cuts(&workspace, cx), Vec::<TrimCut>::new());
    click(cx, "trim-done");
    assert_eq!(bar(&workspace, cx), None);
}

#[gpui::test]
fn a_running_trim_shows_the_ai_bar_above_the_popover(cx: &mut TestAppContext) {
    let (_, cx) = open(cx, Some(two_levels()));
    click(cx, "lab-button");
    assert_eq!(cx.debug_bounds("ai-bar"), None);

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(RunTrim(TrimLevel::SlightTrim)), cx);
    });

    let bar = cx
        .debug_bounds("ai-bar")
        .expect("a running Trim shows the AI bar");
    let popover = cx.debug_bounds("lab-popover").expect("the popover is open");
    assert!(
        bar.bottom() <= popover.top(),
        "the AI bar {bar:?} overlaps the popover {popover:?}"
    );
}
