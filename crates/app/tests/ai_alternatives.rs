use std::ops::Range;
use std::sync::Arc;

use app::{Ai, AiAlternatives, AiStatus, AlternativesPanel, Workspace};
use gpui::{Entity, TestAppContext, VisualTestContext};
use ka_teb_core::{
    AiAdapter, AiError, AiRequest, Document, LabFinding, LabRequest, Provenance, TrimRequest,
};

/// Gives fixed Variant texts, or fails, as an AI backend would.
struct FakeAdapter(Result<Vec<&'static str>, AiError>);

impl AiAdapter for FakeAdapter {
    fn variants(&self, _request: &AiRequest) -> Result<Vec<String>, AiError> {
        let texts = self.0.clone()?;
        Ok(texts.into_iter().map(str::to_owned).collect())
    }

    fn lab_check(&self, _request: &LabRequest) -> Result<Vec<LabFinding>, AiError> {
        Ok(Vec::new())
    }

    fn trim(&self, _request: &TrimRequest) -> Result<Vec<Range<usize>>, AiError> {
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

/// Moves the cursor into "customer" (10..18).
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

fn status(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> AiStatus {
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
    assert_eq!(status(&workspace, cx), AiStatus::Loading);
    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", Provenance::Writer, true)]
    );

    cx.run_until_parked();
    assert_eq!(status(&workspace, cx), AiStatus::Idle);
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
