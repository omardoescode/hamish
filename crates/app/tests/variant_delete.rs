use app::{PlayingEffect, Workspace};
use gpui::{Entity, TestAppContext, VisualTestContext};
use ka_teb_core::{Document, Effect, Level, LoadedSettings, Provenance, Settings, SlotId};

fn open(
    cx: &mut TestAppContext,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

/// "Title\nThe customer waits." with a Word Slot on "customer" (10..18):
/// "customer", "client", "patron".
#[expect(clippy::expect_used, reason = "test fixture")]
fn slot_document() -> (Document, SlotId) {
    let mut document = Document::new("Title\nThe customer waits.");
    let slot = document
        .create_slot(10..18, Level::Word, "client", Provenance::Writer)
        .expect("valid range");
    document
        .add_variant(slot, "patron", Provenance::Ai)
        .expect("the Slot exists");
    (document, slot)
}

/// Moves the caret into "customer".
const INTO_CUSTOMER: &str = "down right right right right right";

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

/// The Variant texts of `slot` and its Active variant index.
fn variants(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    slot: SlotId,
) -> Option<(Vec<String>, usize)> {
    cx.read(|cx| {
        let slot = workspace.read(cx).editor().read(cx).document().slot(slot)?;
        let texts = slot
            .variants
            .iter()
            .map(|variant| variant.text().to_owned())
            .collect();
        Some((texts, slot.active))
    })
}

fn effects(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<PlayingEffect> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).effects())
}

fn effect(effect: Effect, range: std::ops::Range<usize>) -> PlayingEffect {
    PlayingEffect { effect, range }
}

fn strings(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

#[gpui::test]
fn ctrl_shift_backspace_in_a_slot_deletes_the_active_variant_and_the_next_shows(
    cx: &mut TestAppContext,
) {
    let (document, slot) = slot_document();
    let (workspace, cx) = open(cx, document);

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-backspace");

    assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
    assert_eq!(
        variants(&workspace, cx, slot),
        Some((strings(&["client", "patron"]), 0))
    );
    assert_eq!(
        effects(&workspace, cx),
        vec![effect(Effect::Explosion, 10..18)]
    );
}

#[gpui::test]
fn the_caret_at_the_end_of_the_span_is_in_the_slot(cx: &mut TestAppContext) {
    let (document, slot) = slot_document();
    let (workspace, cx) = open(cx, document);

    cx.simulate_keystrokes(
        "down right right right right right right right right right right right right",
    );
    cx.simulate_keystrokes("ctrl-shift-backspace");

    assert_eq!(
        variants(&workspace, cx, slot),
        Some((strings(&["client", "patron"]), 0))
    );
}

#[gpui::test]
fn ctrl_shift_backspace_outside_a_slot_changes_nothing(cx: &mut TestAppContext) {
    let (document, _) = slot_document();
    let (workspace, cx) = open(cx, document);

    cx.simulate_keystrokes("down ctrl-shift-backspace");

    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
    assert_eq!(effects(&workspace, cx), Vec::new());
}

#[gpui::test]
fn undo_puts_the_deleted_variant_back_as_active_with_a_burst(cx: &mut TestAppContext) {
    let (document, slot) = slot_document();
    let (workspace, cx) = open(cx, document);

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("down ctrl-shift-backspace");
    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");

    cx.simulate_keystrokes("ctrl-z");

    assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
    assert_eq!(
        variants(&workspace, cx, slot),
        Some((strings(&["customer", "client", "patron"]), 1))
    );
    assert_eq!(
        effects(&workspace, cx).last(),
        Some(&effect(Effect::UndoBurst, 10..16))
    );
}

#[gpui::test]
fn redo_of_a_delete_plays_the_explosion_again(cx: &mut TestAppContext) {
    let (document, slot) = slot_document();
    let (workspace, cx) = open(cx, document);

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("down ctrl-shift-backspace ctrl-z ctrl-shift-z");

    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
    assert_eq!(
        variants(&workspace, cx, slot),
        Some((strings(&["customer", "patron"]), 1))
    );
    assert_eq!(
        effects(&workspace, cx).last(),
        Some(&effect(Effect::Explosion, 10..16))
    );
}

#[gpui::test]
fn deleting_one_of_two_variants_ends_the_slot_and_undo_brings_it_back(cx: &mut TestAppContext) {
    let mut document = Document::new("Title\nThe customer waits.");
    let slot = document
        .create_slot(10..18, Level::Word, "client", Provenance::Writer)
        .unwrap();
    let (workspace, cx) = open(cx, document);

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-backspace");
    assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
    assert_eq!(variants(&workspace, cx, slot), None);

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(
        variants(&workspace, cx, slot),
        Some((strings(&["customer", "client"]), 0))
    );
}

#[gpui::test]
fn up_and_down_with_the_caret_in_a_slot_cycle_its_variants(cx: &mut TestAppContext) {
    let (document, slot) = slot_document();
    let (workspace, cx) = open(cx, document);

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("down");
    assert_eq!(text(&workspace, cx), "Title\nThe client waits.");

    cx.simulate_keystrokes("up up");
    assert_eq!(text(&workspace, cx), "Title\nThe patron waits.");
    assert_eq!(
        variants(&workspace, cx, slot).map(|(_, active)| active),
        Some(2)
    );
}
fn reduce_motion(cx: &mut VisualTestContext) {
    let loaded = LoadedSettings {
        settings: Settings {
            reduced_motion: true,
            ..Settings::default()
        },
        problem: None,
    };
    cx.update(|_, cx| app::use_settings(None, loaded, cx));
}

#[gpui::test]
fn with_reduced_motion_delete_and_undo_play_no_effect(cx: &mut TestAppContext) {
    let (document, _) = slot_document();
    let (workspace, cx) = open(cx, document);
    reduce_motion(cx);

    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-backspace");
    assert_eq!(text(&workspace, cx), "Title\nThe client waits.");
    assert_eq!(effects(&workspace, cx), Vec::new());

    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&workspace, cx), "Title\nThe customer waits.");
    assert_eq!(effects(&workspace, cx), Vec::new());
}
