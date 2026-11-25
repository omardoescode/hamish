use app::{AlternativesPanel, Workspace};
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};
use ka_teb_core::{Document, Level, Provenance};

fn open(
    cx: &mut TestAppContext,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn panel(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<Entity<AlternativesPanel>> {
    cx.read(|cx| workspace.read(cx).alternatives_panel().cloned())
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<(String, bool)> {
    let panel = panel(workspace, cx).expect("the panel is open");
    cx.read(|cx| {
        panel
            .read(cx)
            .rows(cx)
            .into_iter()
            .map(|row| (row.text, row.active))
            .collect()
    })
}

fn row(text: &str, active: bool) -> (String, bool) {
    (text.to_owned(), active)
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

/// "Title\nThe customer waits." ("customer" is 10..18).
fn customer_document() -> Document {
    Document::new("Title\nThe customer waits.")
}

/// Moves the cursor into "customer".
const INTO_CUSTOMER: &str = "down right right right right right";

#[gpui::test]
fn ctrl_shift_a_on_a_word_opens_the_panel_with_the_word_as_the_original(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a");

    assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);
    let level = panel(&workspace, cx).map(|panel| cx.read(|cx| panel.read(cx).level(cx)));
    assert_eq!(level, Some(Some(Level::Word)));
}

#[gpui::test]
fn return_in_the_last_row_adds_a_variant_and_the_original_stays_active(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a");
    cx.simulate_input("client");
    cx.simulate_keystrokes("enter");

    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", true), row("client", false)]
    );
    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
}

fn focused_on_editor(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    cx.update(|window, cx| editor.focus_handle(cx).is_focused(window))
}

#[gpui::test]
fn escape_closes_the_panel_and_the_editor_takes_the_keys_again(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, customer_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a escape");

    assert!(panel(&workspace, cx).is_none());
    assert!(focused_on_editor(&workspace, cx));
}

/// "Title\nThe customer waits." with a Word Slot on "customer": "customer", "client", "patron".
#[expect(clippy::expect_used, reason = "test fixture")]
fn slot_document() -> Document {
    let mut document = customer_document();
    let slot = document
        .create_slot(10..18, Level::Word, "client", Provenance::Writer)
        .expect("valid range");
    document
        .add_variant(slot, "patron", Provenance::Ai)
        .expect("the Slot exists");
    document
}

#[gpui::test]
fn up_and_down_in_the_panel_show_each_variant_in_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up");
    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");

    cx.simulate_keystrokes("up");
    assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
    assert_eq!(
        rows(&workspace, cx),
        vec![
            row("customer", false),
            row("client", true),
            row("patron", false)
        ]
    );

    cx.simulate_keystrokes("down down");
    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn provenances(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Provenance> {
    let panel = panel(workspace, cx).expect("the panel is open");
    cx.read(|cx| {
        panel
            .read(cx)
            .rows(cx)
            .into_iter()
            .map(|row| row.provenance)
            .collect()
    })
}

#[gpui::test]
fn return_on_a_variant_edits_it_and_an_edited_ai_variant_is_mixed(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up enter backspace backspace backspace backspace");
    cx.simulate_input("rty");
    cx.simulate_keystrokes("enter");

    assert_eq!(text(&workspace, cx), "Title\nThe party waits.");
    assert_eq!(
        provenances(&workspace, cx),
        vec![Provenance::Writer, Provenance::Writer, Provenance::Mixed]
    );
}

#[gpui::test]
fn escape_while_editing_drops_the_edit_and_keeps_the_panel_open(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up enter backspace escape");

    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
    assert!(panel(&workspace, cx).is_some());
}

#[gpui::test]
fn ctrl_backspace_deletes_the_selected_variant_and_the_original_shows(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up ctrl-backspace");

    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", true), row("client", false)]
    );
}

#[gpui::test]
fn the_original_cannot_be_deleted(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up up up ctrl-backspace");

    assert_eq!(rows(&workspace, cx).len(), 3);
}

#[gpui::test]
fn deleting_all_but_one_variant_ends_the_slot_and_the_panel_keeps_its_text(
    cx: &mut TestAppContext,
) {
    let (workspace, cx) = open(cx, slot_document());

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a up ctrl-backspace up ctrl-backspace");
    assert_eq!(rows(&workspace, cx), vec![row("customer", true)]);
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

    cx.simulate_input("buyer");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        rows(&workspace, cx),
        vec![row("customer", true), row("buyer", false)]
    );
}
