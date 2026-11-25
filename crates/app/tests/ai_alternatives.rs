use std::ops::Range;
use std::sync::Arc;

use app::{
    Ai, AiAlternatives, AiAlternativesError, AiFailure, AiStatus, AlternativesPanel, Workspace,
};
use gpui::{
    Bounds, Entity, EntityInputHandler, Keystroke, Modifiers, MouseButton, TestAppContext,
    VisualTestContext, point, px,
};
use ka_teb_core::{
    AiAdapter, AiError, AiRequest, CancelToken, Document, LabFinding, LabRequest, LoadedSettings,
    Provenance, Settings, TrimRequest,
};

/// Gives fixed Variant texts, or fails, as an AI backend would.
struct FakeAdapter(Result<Vec<&'static str>, AiError>);

impl AiAdapter for FakeAdapter {
    fn variants(
        &self,
        _request: &AiRequest,
        _cancel: &CancelToken,
    ) -> Result<Vec<String>, AiError> {
        let texts = self.0.clone()?;
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

fn open(
    cx: &mut TestAppContext,
    ai: Option<FakeAdapter>,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    if let Some(ai) = ai {
        cx.update(|cx| cx.set_global(Ai(Arc::new(ai))));
    }
    let document = Document::new("Title\nThe customer waits.");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

/// Moves the caret into "customer" (10..18).
const INTO_CUSTOMER: &str = "down right right right right right";

fn client_and_patron() -> FakeAdapter {
    FakeAdapter(Ok(vec!["client", "patron"]))
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

#[gpui::test]
fn ai_alternatives_shows_progress_then_ai_variants_under_the_original(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(client_and_patron()));
    cx.simulate_keystrokes(INTO_CUSTOMER);

    cx.update(|window, cx| window.dispatch_action(Box::new(AiAlternatives), cx));
    assert_eq!(status(&workspace, cx), AiStatus::Running);
    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", Provenance::Writer, true)]
    );

    cx.run_until_parked();
    assert_eq!(status(&workspace, cx), AiStatus::Done);
    assert_eq!(
        rows(&workspace, cx),
        vec![
            row("customer", Provenance::Writer, true),
            row("client", Provenance::Ai, false),
            row("patron", Provenance::Ai, false),
        ]
    );
    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
}

#[gpui::test]
fn a_failed_request_shows_why_and_changes_nothing(cx: &mut TestAppContext) {
    let failing = FakeAdapter(Err(AiError::Declined("no".to_owned())));
    let (workspace, cx) = open(cx, Some(failing));
    cx.simulate_keystrokes(INTO_CUSTOMER);

    cx.simulate_keystrokes("ctrl-shift-g");

    assert_eq!(
        status(&workspace, cx),
        AiStatus::Failed(AiFailure::Ai(AiError::Declined("no".to_owned())))
    );
    assert!(cx.debug_bounds("alternatives-ai-status").is_some());
    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", Provenance::Writer, true)]
    );
    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
}

#[gpui::test]
fn the_empty_row_dances_while_the_ai_works_as_in_a_quick_fill(cx: &mut TestAppContext) {
    let (_workspace, cx) = open(cx, Some(client_and_patron()));
    cx.simulate_keystrokes(INTO_CUSTOMER);

    cx.update(|window, cx| window.dispatch_action(Box::new(AiAlternatives), cx));
    cx.update(|window, _| window.refresh());

    assert!(cx.debug_bounds("alternatives-loading").is_some());
    assert!(cx.debug_bounds("alternatives-loading-still").is_none());
    assert!(cx.debug_bounds("alternatives-ai-status").is_none());
    assert!(cx.debug_bounds("alternatives-row-1").is_none());
}

#[gpui::test]
fn with_reduced_motion_the_loading_row_is_still(cx: &mut TestAppContext) {
    let (_workspace, cx) = open(cx, Some(client_and_patron()));
    let settings = LoadedSettings {
        settings: Settings {
            reduced_motion: true,
            ..Settings::default()
        },
        problem: None,
    };
    cx.update(|_, cx| app::use_settings(None, settings, cx));
    cx.simulate_keystrokes(INTO_CUSTOMER);

    cx.update(|window, cx| window.dispatch_action(Box::new(AiAlternatives), cx));
    cx.update(|window, _| window.refresh());

    assert!(cx.debug_bounds("alternatives-loading-still").is_some());
}

#[gpui::test]
fn one_undo_removes_the_whole_ai_request(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(client_and_patron()));
    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-g");
    assert_eq!(rows(&workspace, cx).len(), 3);

    cx.simulate_keystrokes("escape ctrl-z");

    let slots = cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .slots()
            .len()
    });
    assert_eq!(slots, 0);
    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
}

#[gpui::test]
fn ai_variants_can_be_removed_one_by_one(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(client_and_patron()));
    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-g");

    cx.simulate_keystrokes("up ctrl-backspace");

    assert_eq!(
        rows(&workspace, cx),
        vec![
            row("customer", Provenance::Writer, true),
            row("client", Provenance::Ai, false),
        ]
    );
    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
}

#[gpui::test]
fn ctrl_shift_g_in_the_panel_adds_ai_variants_to_the_slot_it_shows(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(client_and_patron()));
    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up");
    cx.simulate_input("buyer");
    cx.simulate_keystrokes("enter up");
    assert_eq!(text(&workspace, cx), "Title\nThe buyer waits.");

    cx.simulate_keystrokes("ctrl-shift-g");

    assert_eq!(
        rows(&workspace, cx),
        vec![
            row("customer", Provenance::Writer, false),
            row("buyer", Provenance::Writer, true),
            row("client", Provenance::Ai, false),
            row("patron", Provenance::Ai, false),
        ]
    );
    assert_eq!(text(&workspace, cx), "Title\nThe buyer waits.");
}

#[gpui::test]
fn escape_while_the_ai_works_stops_the_request_and_keeps_the_panel_open(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(client_and_patron()));
    cx.simulate_keystrokes(INTO_CUSTOMER);

    cx.update(|window, cx| window.dispatch_action(Box::new(AiAlternatives), cx));
    cx.update(|window, cx| {
        let escape = Keystroke::parse("escape").expect("a valid keystroke");
        window.dispatch_keystroke(escape, cx)
    });
    cx.run_until_parked();

    assert_eq!(status(&workspace, cx), AiStatus::Idle);
    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", Provenance::Writer, true)]
    );
    let slots = cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .slots()
            .len()
    });
    assert_eq!(slots, 0);
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn right_click_on(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, offset: usize) {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    let bounds = cx
        .update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.bounds_for_range(offset..offset, Bounds::default(), window, cx)
            })
        })
        .expect("the editor painted the text");
    let position = bounds.origin + point(px(3.), bounds.size.height / 2.);
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::none());
}

#[gpui::test]
fn the_context_menu_row_asks_the_ai_for_the_word(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Some(client_and_patron()));
    right_click_on(&workspace, cx, 13);

    let menu_row = cx
        .debug_bounds("context-menu-row-1")
        .expect("the menu is open");
    cx.simulate_click(menu_row.center(), Modifiers::none());

    assert_eq!(
        rows(&workspace, cx),
        vec![
            row("customer", Provenance::Writer, true),
            row("client", Provenance::Ai, false),
            row("patron", Provenance::Ai, false),
        ]
    );
}
