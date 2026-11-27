mod system_text;

use std::ops::Range;
use std::sync::Arc;

use app::{Ai, LabStatus, Theme, Workspace};
use gpui::{Entity, TestAppContext, VisualTestContext};
use hamish_core::{
    AiAdapter, AiError, AiRequest, CancelToken, Document, LabFinding, LabRequest, Level,
    LoadedSettings, OnOff, Provenance, Settings, TrimRequest,
};

/// Gives no Variants and no Lab findings, and the same Trim cuts for each level.
struct FakeAdapter(Vec<Range<usize>>);

impl AiAdapter for FakeAdapter {
    fn variants(&self, _: &AiRequest, _: &CancelToken) -> Result<Vec<String>, AiError> {
        Ok(Vec::new())
    }

    fn lab_check(&self, _: &LabRequest, _: &CancelToken) -> Result<Vec<LabFinding>, AiError> {
        Ok(Vec::new())
    }

    fn trim(&self, _: &TrimRequest, _: &CancelToken) -> Result<Vec<Range<usize>>, AiError> {
        Ok(self.0.clone())
    }
}

fn open_document(
    cx: &mut TestAppContext,
    document: Document,
    cuts: Vec<Range<usize>>,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    cx.update(|cx| cx.set_global(Ai(Arc::new(FakeAdapter(cuts)))));
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.update(|_, cx| {
        app::use_settings(
            None,
            LoadedSettings {
                settings: Settings {
                    vim_mode: OnOff::On,
                    ..Settings::default()
                },
                problem: None,
            },
            cx,
        );
    });
    cx.run_until_parked();
    (workspace, cx)
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

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range<usize> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).selection())
}

/// "one two three" with "two" as a Word Slot: "two", "2", "deux"; "two" shows.
#[expect(clippy::unwrap_used, reason = "test fixture")]
fn slot_on_two() -> Document {
    let mut document = Document::new("one two three");
    let slot = document
        .create_slot(4..7, Level::Word, "2", Provenance::Writer)
        .unwrap();
    document
        .add_variant(slot, "deux", Provenance::Writer)
        .unwrap();
    document.set_active(slot, 0).unwrap();
    document
}

fn panel_rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    cx.read(|cx| {
        workspace.read(cx).alternatives_panel().map(|panel| {
            panel
                .read(cx)
                .rows(cx)
                .into_iter()
                .map(|row| row.text)
                .collect()
        })
    })
}

#[test]
fn in_the_alternatives_panel_j_k_move_rows_and_enter_closes_on_the_active_variant() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open_document(cx, slot_on_two(), Vec::new());

        cx.simulate_keystrokes("w space a");
        cx.run_until_parked();
        cx.simulate_keystrokes("k k k j");
        assert_eq!(text(&workspace, cx), "one 2 three");

        cx.simulate_keystrokes("enter");
        assert_eq!(panel_rows(&workspace, cx), None);
        assert_eq!(text(&workspace, cx), "one 2 three");
    });
}

#[test]
fn in_the_alternatives_panel_dd_deletes_the_variant_and_a_digit_chooses_the_level() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open_document(cx, slot_on_two(), Vec::new());

        cx.simulate_keystrokes("w space a k d d");
        cx.run_until_parked();
        assert_eq!(
            panel_rows(&workspace, cx),
            Some(vec!["two".to_owned(), "2".to_owned()])
        );

        cx.simulate_keystrokes("2");
        let level = cx.read(|cx| {
            workspace
                .read(cx)
                .alternatives_panel()
                .and_then(|panel| panel.read(cx).level(cx))
        });
        assert_eq!(level, Some(Level::Sentence));
    });
}

#[test]
fn in_the_alternatives_panel_i_types_in_the_row_and_escape_goes_back_to_the_rows() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open_document(cx, slot_on_two(), Vec::new());

        cx.simulate_keystrokes("w space a k i s escape");
        assert_eq!(
            panel_rows(&workspace, cx),
            Some(vec!["two".to_owned(), "2".to_owned(), "deuxs".to_owned()])
        );

        cx.simulate_keystrokes("j i t r o i s enter escape k");
        assert_eq!(
            panel_rows(&workspace, cx),
            Some(vec![
                "two".to_owned(),
                "2".to_owned(),
                "deuxs".to_owned(),
                "trois".to_owned()
            ])
        );

        cx.simulate_keystrokes("escape");
        assert_eq!(panel_rows(&workspace, cx), None);
    });
}

fn overflow_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .overflow()
            .text()
            .to_owned()
    })
}

fn overflow_status(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.read(|cx| workspace.read(cx).overflow_panel().read(cx).vim_status(cx))
}

#[test]
fn overflow_has_the_vim_keys_and_ctrl_w_moves_between_it_and_the_editor() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open_document(cx, Document::new("ab"), Vec::new());

        cx.simulate_keystrokes("ctrl-shift-o i");
        cx.simulate_input("one two");
        cx.simulate_keystrokes("escape b d w");
        assert_eq!(overflow_text(&workspace, cx), "one ");
        assert_eq!(overflow_status(&workspace, cx).as_deref(), Some("NORMAL"));

        cx.simulate_keystrokes("ctrl-w h x");
        assert_eq!(text(&workspace, cx), "b");
        assert_eq!(overflow_text(&workspace, cx), "one ");

        cx.simulate_keystrokes("ctrl-w l x");
        assert_eq!(overflow_text(&workspace, cx), "one");
    });
}

#[test]
fn escape_in_overflow_insert_mode_goes_to_normal_mode_and_keeps_the_keyboard() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open_document(cx, Document::new("ab"), Vec::new());

        cx.simulate_keystrokes("ctrl-shift-o i");
        cx.simulate_input("xy");
        cx.simulate_keystrokes("escape x");
        assert_eq!(overflow_text(&workspace, cx), "x");

        cx.simulate_keystrokes("escape x");
        assert_eq!(text(&workspace, cx), "b");
    });
}

#[test]
fn space_p_puts_the_overflow_block_at_the_editor_caret_and_p_stays_the_register_put() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open_document(cx, Document::new("ab"), Vec::new());
        cx.simulate_keystrokes("ctrl-shift-o i");
        cx.simulate_input("block");
        cx.simulate_keystrokes("escape ctrl-w h");

        cx.simulate_keystrokes("space p");
        assert_eq!(text(&workspace, cx), "block ab");

        cx.simulate_keystrokes("u x p");
        assert_eq!(text(&workspace, cx), "ba");
    });
}

/// "one two three four" with Trim cuts over "two" (4..7) and "four" (14..18).
fn trimmed(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (workspace, cx) =
        open_document(cx, Document::new("one two three four"), vec![4..7, 14..18]);
    cx.simulate_keystrokes("space t");
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).lab().read(cx).trim_bar(cx).is_some()),
        "the Trim bar shows"
    );
    (workspace, cx)
}

fn trim_open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .trim_proposal()
            .is_some()
    })
}

#[test]
fn in_a_trim_n_walks_the_cuts_space_k_keeps_one_and_enter_makes_the_rest() {
    system_text::run(1, |cx| {
        let (workspace, cx) = trimmed(cx);

        cx.simulate_keystrokes("n");
        assert_eq!(selection(&workspace, cx), 4..4);
        cx.simulate_keystrokes("space k n");
        assert_eq!(selection(&workspace, cx), 14..14);
        cx.simulate_keystrokes("shift-n");
        assert_eq!(selection(&workspace, cx), 14..14);

        cx.simulate_keystrokes("enter");
        assert_eq!(text(&workspace, cx), "one two three ");
        assert!(!trim_open(&workspace, cx));
    });
}

#[test]
fn in_a_trim_escape_closes_the_trim_bar_and_the_text_stays() {
    system_text::run(1, |cx| {
        let (workspace, cx) = trimmed(cx);

        cx.simulate_keystrokes("escape");

        assert!(!trim_open(&workspace, cx));
        assert_eq!(text(&workspace, cx), "one two three four");
    });
}

fn lab_statuses(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<LabStatus> {
    cx.read(|cx| {
        workspace
            .read(cx)
            .lab()
            .read(cx)
            .rows(cx)
            .into_iter()
            .map(|row| row.status)
            .collect()
    })
}

#[test]
fn in_the_lab_j_k_move_between_checks_and_enter_runs_the_check() {
    system_text::run(1, |cx| {
        let (workspace, cx) = open_document(cx, Document::new("one two"), Vec::new());

        cx.simulate_keystrokes("space l j j j k enter");
        cx.run_until_parked();

        let statuses = lab_statuses(&workspace, cx);
        assert_eq!(statuses.get(1), Some(&LabStatus::Idle));
        assert_eq!(statuses.get(2), Some(&LabStatus::Marked(0)));
        assert_eq!(statuses.get(3), Some(&LabStatus::Idle));

        cx.simulate_keystrokes("escape");
        assert!(!cx.read(|cx| workspace.read(cx).lab().read(cx).is_open()));
    });
}

fn theme(cx: &mut VisualTestContext) -> Theme {
    cx.read(|cx| cx.global::<Theme>().clone())
}

#[test]
fn in_settings_j_k_move_fields_and_h_l_change_the_value() {
    system_text::run(1, |cx| {
        let (_, cx) = open_document(cx, Document::new("one"), Vec::new());
        let hint = theme(cx).font_size_hint;

        cx.simulate_keystrokes("ctrl-, j j l");
        assert!(theme(cx).font_size_hint > hint, "a larger UI scale");

        cx.simulate_keystrokes("h");
        assert_eq!(theme(cx).font_size_hint, hint);
    });
}

#[test]
fn in_settings_enter_opens_a_select_where_j_moves_rows_and_letters_search() {
    system_text::run(1, |cx| {
        let (_, cx) = open_document(cx, Document::new("one"), Vec::new());
        let first = theme(cx);

        cx.simulate_keystrokes("ctrl-, enter");
        assert!(cx.debug_bounds("settings-select-search").is_some());

        cx.simulate_keystrokes("j enter");
        assert!(cx.debug_bounds("settings-select-search").is_none());
        let second = theme(cx);
        assert_ne!(second.background, first.background);

        cx.simulate_keystrokes("enter q q q");
        assert!(cx.debug_bounds("settings-select-no-match").is_some());
        cx.simulate_keystrokes("escape escape x");
        assert_eq!(theme(cx).background, second.background);
    });
}
