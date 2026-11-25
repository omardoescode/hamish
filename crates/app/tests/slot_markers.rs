use app::{Marker, MarkerKind, Workspace};
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};
use ka_teb_core::{Document, Level, Provenance, SlotId};

fn open(
    cx: &mut TestAppContext,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
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
    bounds.origin + point(px(3.), bounds.size.height / 2.)
}

fn hover(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, offset: usize) {
    let position = point_at(workspace, cx, offset);
    cx.simulate_mouse_move(position, None, Modifiers::none());
}

fn hovered_slot(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<SlotId> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).hovered_slot())
}

/// "Title\nIt is a tension here." with a Word Slot on "tension" (Variants "tension", "struggle").
#[expect(clippy::expect_used, reason = "test fixture")]
fn word_slot_document() -> (Document, SlotId) {
    let mut document = Document::new("Title\nIt is a tension here.");
    let slot = document
        .create_slot(14..21, Level::Word, "struggle", Provenance::Writer)
        .expect("valid range");
    (document, slot)
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

fn counter(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| workspace.read(cx).editor().read(cx).counter_label())
}

/// "Title\nIt is an eraser here." with a Word Slot on "eraser" (Variants "eraser", "thumbtack").
#[expect(clippy::expect_used, reason = "test fixture")]
fn article_document() -> Document {
    let mut document = Document::new("Title\nIt is an eraser here.");
    document
        .create_slot(15..21, Level::Word, "thumbtack", Provenance::Writer)
        .expect("valid range");
    document
}

#[gpui::test]
fn down_over_a_slot_shows_the_next_variant_with_article_agreement(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, article_document());

    hover(&workspace, cx, 17);
    cx.simulate_keystrokes("down");

    assert_eq!(text(&workspace, cx), "Title\nIt is a thumbtack here.");
    assert_eq!(counter(&workspace, cx), "6 words 29 chars");
}

#[gpui::test]
fn up_over_a_slot_shows_the_previous_variant_and_wraps(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, article_document());

    hover(&workspace, cx, 17);
    cx.simulate_keystrokes("up");
    assert_eq!(text(&workspace, cx), "Title\nIt is a thumbtack here.");

    cx.simulate_keystrokes("up");
    assert_eq!(text(&workspace, cx), "Title\nIt is an eraser here.");
}

#[gpui::test]
fn undo_reverts_a_variant_switch(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, article_document());

    hover(&workspace, cx, 17);
    cx.simulate_keystrokes("down ctrl-z");

    assert_eq!(text(&workspace, cx), "Title\nIt is an eraser here.");
}

#[gpui::test]
fn the_pointer_over_a_slot_hovers_it(cx: &mut TestAppContext) {
    let (document, slot) = word_slot_document();
    let (workspace, cx) = open(cx, document);

    hover(&workspace, cx, 16);
    assert_eq!(hovered_slot(&workspace, cx), Some(slot));

    hover(&workspace, cx, 7);
    assert_eq!(hovered_slot(&workspace, cx), None);
}

fn markers(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Marker> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).markers().to_vec())
}

#[gpui::test]
fn a_word_slot_shows_an_underline_with_one_dot_per_variant(cx: &mut TestAppContext) {
    let (document, slot) = word_slot_document();
    let (workspace, cx) = open(cx, document);

    assert_eq!(
        markers(&workspace, cx),
        vec![Marker {
            slot,
            kind: MarkerKind::Underline,
            dots: 2,
            active: 0,
        }]
    );

    hover(&workspace, cx, 16);
    cx.simulate_keystrokes("down");
    assert_eq!(markers(&workspace, cx)[0].active, 1);
}

/// "Title\nOld text here." with a Paragraph Slot (Variants "Old text here.", "New words
/// here.", "Third.") and a Word Slot on "words" nested in the second Variant.
#[expect(clippy::expect_used, reason = "test fixture")]
fn nested_document() -> (Document, SlotId, SlotId) {
    let mut document = Document::new("Title\nOld text here.");
    let paragraph = document
        .create_slot(
            6..20,
            Level::Paragraph,
            "New words here.",
            Provenance::Writer,
        )
        .expect("valid range");
    document
        .add_variant(paragraph, "Third.", Provenance::Ai)
        .expect("visible Slot");
    document.set_active(paragraph, 1).expect("Variant 1 exists");
    let word = document
        .create_slot(10..15, Level::Word, "lines", Provenance::Writer)
        .expect("valid range");
    document.set_active(paragraph, 0).expect("Variant 0 exists");
    (document, paragraph, word)
}

#[gpui::test]
fn nested_slot_markers_show_inside_the_active_paragraph_variant(cx: &mut TestAppContext) {
    let (document, paragraph, word) = nested_document();
    let (workspace, cx) = open(cx, document);
    let paragraph_marker = |active| Marker {
        slot: paragraph,
        kind: MarkerKind::MarginLine,
        dots: 3,
        active,
    };

    assert_eq!(markers(&workspace, cx), vec![paragraph_marker(0)]);

    hover(&workspace, cx, 8);
    cx.simulate_keystrokes("down");
    assert_eq!(text(&workspace, cx), "Title\nNew words here.");
    assert_eq!(
        markers(&workspace, cx),
        vec![
            paragraph_marker(1),
            Marker {
                slot: word,
                kind: MarkerKind::Underline,
                dots: 2,
                active: 0,
            },
        ]
    );
}

#[gpui::test]
fn the_innermost_slot_under_the_pointer_is_hovered(cx: &mut TestAppContext) {
    let (mut document, paragraph, word) = nested_document();
    document.set_active(paragraph, 1).expect("Variant 1 exists");
    let (workspace, cx) = open(cx, document);

    hover(&workspace, cx, 11);
    assert_eq!(hovered_slot(&workspace, cx), Some(word));

    hover(&workspace, cx, 7);
    assert_eq!(hovered_slot(&workspace, cx), Some(paragraph));
}

#[gpui::test]
fn up_down_cycle_word_slots_on_and_under_a_markdown_heading(cx: &mut TestAppContext) {
    let mut document = Document::new("# A tension here\nIt is **bold** and quiet.");
    let in_heading = document
        .create_slot(4..11, Level::Word, "struggle", Provenance::Writer)
        .unwrap();
    let in_body = document
        .create_slot(36..41, Level::Word, "calm", Provenance::Writer)
        .unwrap();
    let (workspace, cx) = open(cx, document);

    hover(&workspace, cx, 6);
    assert_eq!(hovered_slot(&workspace, cx), Some(in_heading));
    cx.simulate_keystrokes("down");
    assert_eq!(
        text(&workspace, cx),
        "# A struggle here\nIt is **bold** and quiet."
    );

    hover(&workspace, cx, 38);
    assert_eq!(hovered_slot(&workspace, cx), Some(in_body));
    cx.simulate_keystrokes("down");
    assert_eq!(
        text(&workspace, cx),
        "# A struggle here\nIt is **bold** and calm."
    );
}

fn underline(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    slot: SlotId,
) -> Vec<Point<Pixels>> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).underline(slot))
}

#[gpui::test]
fn a_slot_underline_is_wavy_and_the_same_after_an_edit_elsewhere(cx: &mut TestAppContext) {
    let (document, slot) = word_slot_document();
    let (workspace, cx) = open(cx, document);

    let before = underline(&workspace, cx, slot);
    assert!(before.len() > 2, "the underline has points along the Slot");
    assert!(
        before.iter().any(|at| at.y != before[0].y),
        "the underline is not a straight line"
    );

    cx.simulate_input("My ");
    assert_eq!(text(&workspace, cx), "My Title\nIt is a tension here.");
    assert_eq!(underline(&workspace, cx, slot), before);
}

/// "Title\n" then a paragraph of 3 sentences, each long enough to wrap, with a Sentence
/// Slot on the middle one.
const THREE_SENTENCES: &str = "Title\nThe first sentence of this paragraph runs long enough to wrap onto a second row of text in the column. The middle sentence is the one with Variants and it also runs long enough to wrap onto rows. The last sentence ends the paragraph after the Slot.";
const MIDDLE_SENTENCE: std::ops::Range<usize> = 109..201;

#[expect(clippy::expect_used, reason = "test fixture")]
fn sentence_slot_document() -> (Document, SlotId) {
    let mut document = Document::new(THREE_SENTENCES);
    let slot = document
        .create_slot(
            MIDDLE_SENTENCE,
            Level::Sentence,
            "Short one.",
            Provenance::Writer,
        )
        .expect("valid range");
    (document, slot)
}

#[expect(clippy::expect_used, reason = "test fixture")]
/// The window bounds of the caret box at byte `offset`: its row top and height.
fn caret_box(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    offset: usize,
) -> Bounds<Pixels> {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.bounds_for_range(offset..offset, Bounds::default(), window, cx)
        })
    })
    .expect("the editor painted the text")
}

fn marker_dots(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    slot: SlotId,
) -> Vec<Bounds<Pixels>> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).marker_dots(slot))
}

#[gpui::test]
fn a_sentence_slot_mid_paragraph_shows_a_sentence_marker_with_dots_after_its_end(
    cx: &mut TestAppContext,
) {
    let (document, slot) = sentence_slot_document();
    let (workspace, cx) = open(cx, document);

    assert_eq!(
        markers(&workspace, cx),
        vec![Marker {
            slot,
            kind: MarkerKind::SentenceUnderline,
            dots: 2,
            active: 0,
        }]
    );
    let end = caret_box(&workspace, cx, MIDDLE_SENTENCE.end);
    let next_sentence = caret_box(&workspace, cx, MIDDLE_SENTENCE.end + 5);
    let dots = marker_dots(&workspace, cx, slot);
    assert_eq!(dots.len(), 2);
    for dot in &dots {
        assert!(
            dot.left() >= end.left(),
            "dot {dot:?} is after the end {end:?}"
        );
        assert!(
            dot.left() < next_sentence.left(),
            "dot {dot:?} is before the next sentence"
        );
        assert!(
            dot.top() >= end.top() && dot.bottom() <= end.bottom(),
            "dot {dot:?} is on the last row {end:?}"
        );
    }
}

#[gpui::test]
fn a_sentence_slot_underline_covers_only_the_rows_of_the_sentence(cx: &mut TestAppContext) {
    let (document, slot) = sentence_slot_document();
    let (workspace, cx) = open(cx, document);
    let start = caret_box(&workspace, cx, MIDDLE_SENTENCE.start);
    let end = caret_box(&workspace, cx, MIDDLE_SENTENCE.end);
    assert!(end.top() > start.top(), "the sentence wraps");

    let line = underline(&workspace, cx, slot);
    let (first, last) = (line[0], line[line.len() - 1]);
    assert_eq!(first.x, start.left());
    assert!(first.y > start.top() && first.y < start.bottom());
    assert_eq!(last.x, end.left());
    assert!(last.y > end.top() && last.y < end.bottom());
    assert!(
        line.iter()
            .all(|at| at.y > start.top() && at.y < end.bottom()),
        "no stroke on rows of the other sentences"
    );
}

fn hover_tint(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Bounds<Pixels>> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).hover_tint())
}

#[gpui::test]
fn hovering_a_sentence_slot_tints_only_the_sentence(cx: &mut TestAppContext) {
    let (document, _) = sentence_slot_document();
    let (workspace, cx) = open(cx, document);
    let start = caret_box(&workspace, cx, MIDDLE_SENTENCE.start);
    let end = caret_box(&workspace, cx, MIDDLE_SENTENCE.end);

    hover(&workspace, cx, MIDDLE_SENTENCE.start + 3);
    cx.run_until_parked();
    let tint = hover_tint(&workspace, cx);

    assert!(tint.len() >= 2, "one box per row: {tint:?}");
    let (first, last) = (tint[0], tint[tint.len() - 1]);
    assert!(first.left() > start.left() - px(4.) && first.left() < start.left());
    assert_eq!(first.top(), start.top());
    assert!(last.right() > end.left() && last.right() < end.left() + px(4.));
    assert_eq!(last.bottom(), end.bottom());

    hover(&workspace, cx, 10);
    cx.run_until_parked();
    assert_eq!(hover_tint(&workspace, cx), Vec::new());
}

#[gpui::test]
fn the_sentence_slot_the_alternatives_panel_shows_keeps_its_tint(cx: &mut TestAppContext) {
    let (document, _) = sentence_slot_document();
    let (workspace, cx) = open(cx, document);

    let inside = point_at(&workspace, cx, MIDDLE_SENTENCE.start + 3);
    cx.simulate_click(inside, Modifiers::none());
    cx.simulate_keystrokes("ctrl-shift-a");
    hover(&workspace, cx, 10);
    cx.run_until_parked();
    let start = caret_box(&workspace, cx, MIDDLE_SENTENCE.start);
    let end = caret_box(&workspace, cx, MIDDLE_SENTENCE.end);
    let tint = hover_tint(&workspace, cx);

    assert!(tint.len() >= 2, "one box per row: {tint:?}");
    assert_eq!(tint[0].top(), start.top());
    assert_eq!(tint[tint.len() - 1].bottom(), end.bottom());
}
