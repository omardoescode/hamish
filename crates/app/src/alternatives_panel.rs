//! The Alternatives panel: it renders the Alternatives list of the editor target, runs
//! its AI requests and Quick fill reveal, and plays its sounds and explosions.

use std::time::Duration;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    Modifiers, Render, ScrollHandle, Subscription, Task, Window, canvas, div, prelude::*, px,
};
use hamish_core::{
    AiAlternativesError, AiRequest, AiTrigger, AlternativesList, Document, Effect, Level,
    ListEditor, ListOutcome, ListTarget, Provenance, SlotError, SlotId, VariantRow,
};

use crate::actions::{
    ClosePanel, ConfirmVariant, DeleteBackward, DeleteVariant, Paste, Redo, SelectNextVariant,
    SelectPreviousVariant, Undo,
};
use crate::ai::{AiRun, AiStatus};
use crate::chrome::selectable_row;
use crate::editor::Spotlight;
use crate::effects::EffectPlayer;
use crate::sounds;
use crate::{Editor, Theme};
use crate::{loading, settings};

/// Alternatives list text size.
const LIST_FONT_SIZE: f32 = 12.;

/// Time between two Quick fill batches.
const QUICK_FILL_BATCH_GAP: Duration = Duration::from_millis(150);

fn ai_run(panel: &mut AlternativesPanel, (): ()) -> &mut AiRun<(), AiAlternativesError> {
    &mut panel.ai
}

/// The [`Editor`] as the Alternatives list sees it: each change is one editor undo step.
struct EditorChanges<'a> {
    editor: &'a Entity<Editor>,
    cx: &'a mut App,
}

impl ListEditor for EditorChanges<'_> {
    fn document(&self) -> &Document {
        self.editor.read(self.cx).document()
    }

    fn change_document<T, E>(
        &mut self,
        change: impl FnOnce(&mut Document) -> Result<T, E>,
    ) -> Result<T, E> {
        self.editor
            .update(self.cx, |editor, cx| editor.change_document(change, cx))
    }

    fn delete_variant(
        &mut self,
        id: SlotId,
        index: usize,
    ) -> Result<Option<std::ops::Range<usize>>, SlotError> {
        self.editor
            .update(self.cx, |editor, cx| editor.delete_variant(id, index, cx))
    }
}

/// The left Alternatives panel. It changes the Document only through
/// the [`Editor`], so each change is one editor undo step.
pub struct AlternativesPanel {
    editor: Entity<Editor>,
    focus_handle: FocusHandle,
    list: AlternativesList,
    /// The row that the last delete blew up.
    explosion_row: usize,
    explosions: EffectPlayer,
    /// The AI request of the context menu or of a Quick fill; it stops when the panel closes.
    ai: AiRun<(), AiAlternativesError>,
    /// The batch reveal of Quick fill results. Dropping it stops the reveal.
    reveal: Option<Task<()>>,
    scroll: ScrollHandle,
    _editor_changes: Subscription,
}

impl AlternativesPanel {
    pub(crate) fn new(editor: Entity<Editor>, cx: &mut Context<Self>) -> Self {
        let editor_changes = cx.observe(&editor, |panel, _, cx| {
            if panel.list.follow_document(panel.editor.read(cx).document()) {
                panel.show_spotlight(cx);
                panel.sync_preview(cx);
            }
            cx.notify();
        });
        let list = {
            let editor = editor.read(cx);
            AlternativesList::new(editor.document(), editor.target())
        };
        let panel = Self {
            editor,
            focus_handle: cx.focus_handle(),
            list,
            explosion_row: 0,
            explosions: EffectPlayer::default(),
            ai: AiRun::default(),
            reveal: None,
            scroll: ScrollHandle::new(),
            _editor_changes: editor_changes,
        };
        panel.show_spotlight(cx);
        panel
    }

    fn document<'a>(&self, cx: &'a App) -> &'a Document {
        self.editor.read(cx).document()
    }

    /// The paragraph of the list target stays bright in the editor.
    fn show_spotlight(&self, cx: &mut App) {
        let spotlight = match self.list.target() {
            ListTarget::Slot(id) => Some(Spotlight::Slot(*id)),
            ListTarget::Text { range, .. } => Some(Spotlight::Text(range.clone())),
            ListTarget::Nothing => None,
        };
        self.editor
            .update(cx, |editor, cx| editor.set_spotlight(spotlight, cx));
    }

    /// The draft of the last row shows in the Document in place of the list target.
    fn sync_preview(&self, cx: &mut App) {
        let preview = self
            .list
            .preview(self.document(cx))
            .map(|(range, draft)| (range, draft.to_owned()));
        self.editor.update(cx, |editor, cx| {
            editor.set_preview(
                preview
                    .as_ref()
                    .map(|(range, draft)| (range.clone(), draft.as_str())),
                cx,
            );
        });
    }

    /// Does what the list asks after a writer input.
    fn apply(&mut self, outcome: ListOutcome, cx: &mut Context<Self>) {
        self.show_spotlight(cx);
        if outcome.preview {
            self.sync_preview(cx);
        }
        if let Some(switch) = outcome.switch {
            sounds::play(switch.sound(), cx);
        }
        if outcome.scroll {
            self.scroll_to_row(cx);
        }
        if outcome.quick_fill {
            self.ask_ai(AiTrigger::QuickFill, cx);
        }
        cx.notify();
    }

    /// Shows Slot `id`, with the last row selected.
    pub(crate) fn show_slot(&mut self, id: SlotId, cx: &mut Context<Self>) {
        let outcome = self.list.show_slot(self.editor.read(cx).document(), id);
        self.apply(outcome, cx);
    }

    /// The Level tab that is active. `None` with nothing to show.
    #[must_use]
    pub fn level(&self, cx: &App) -> Option<Level> {
        self.list.level(self.document(cx))
    }

    /// The row where a delete explosion plays now. The next row has moved up into it.
    #[must_use]
    pub fn exploding_row(&self, cx: &App) -> Option<usize> {
        (!self.explosions.playing(cx).is_empty()).then_some(self.explosion_row)
    }

    /// The Variants in order, the Original first. Quick fill results not shown yet
    /// are left out.
    #[must_use]
    pub fn rows(&self, cx: &App) -> Vec<VariantRow> {
        self.list.rows(self.document(cx))
    }
}

impl AlternativesPanel {
    /// Where the AI request of the panel is. A Quick fill runs until its last batch shows.
    #[must_use]
    pub fn ai_status(&self) -> AiStatus<AiAlternativesError> {
        if self.list.is_revealing() {
            return AiStatus::Running;
        }
        self.ai.status().clone()
    }

    /// The AI works, or Quick fill results still come in.
    fn ai_busy(&self) -> bool {
        self.ai.is_running() || self.list.is_revealing()
    }

    /// Asks the AI for Variants of the list target on the background
    /// executor. A request that runs already is not started again.
    pub(crate) fn request_ai(&mut self, cx: &mut Context<Self>) {
        if !self.ai_busy() {
            self.ask_ai(AiTrigger::ContextMenu, cx);
        }
    }

    fn ask_ai(&mut self, trigger: AiTrigger, cx: &mut Context<Self>) {
        let request = self.list.ai_request(self.document(cx), trigger);
        self.ai.start(
            (),
            request,
            |ai, request, cancel| ai.variants(request, cancel),
            ai_run,
            move |panel, request, texts, cx| panel.ai_answered(request, &texts, cx),
            cx,
        );
    }

    /// Adds the AI texts as AI Variants in one undo step. A Quick fill then shows them in
    /// batches under the Variants.
    fn ai_answered(
        &mut self,
        request: &AiRequest,
        texts: &[String],
        cx: &mut Context<Self>,
    ) -> Result<(), AiAlternativesError> {
        let mut editor = EditorChanges {
            editor: &self.editor,
            cx,
        };
        self.list
            .answered(&mut editor, request, texts)
            .map_err(AiAlternativesError::Apply)?;
        self.show_spotlight(cx);
        if request.trigger() == AiTrigger::QuickFill {
            self.start_reveal(cx);
        }
        Ok(())
    }

    /// Shows the Variants a Quick fill added in batches.
    fn start_reveal(&mut self, cx: &mut Context<Self>) {
        self.reveal = Some(cx.spawn(async move |this, cx| {
            loop {
                let more = this.update(cx, AlternativesPanel::reveal_batch);
                if !matches!(more, Ok(true)) {
                    return;
                }
                cx.background_executor().timer(QUICK_FILL_BATCH_GAP).await;
            }
        }));
        cx.notify();
    }

    /// Shows the next batch of Quick fill results. `false` when none are left: the Quick
    /// fill is done.
    fn reveal_batch(&mut self, cx: &mut Context<Self>) -> bool {
        let more = self.list.reveal_batch(self.editor.read(cx).document());
        if !more {
            self.reveal = None;
        }
        cx.notify();
        more
    }
}

impl AlternativesPanel {
    /// Return saves the edit in progress, starts an edit on a Variant row,
    /// or adds the draft of the last row as a Variant.
    fn confirm(&mut self, _: &ConfirmVariant, _: &mut Window, cx: &mut Context<Self>) {
        if self.ai_busy() {
            return;
        }
        let mut editor = EditorChanges {
            editor: &self.editor,
            cx,
        };
        let outcome = self.list.confirm(&mut editor);
        self.apply(outcome, cx);
    }

    fn select_previous(
        &mut self,
        _: &SelectPreviousVariant,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select(self.list.row().saturating_sub(1), cx);
    }

    fn select_next(&mut self, _: &SelectNextVariant, _: &mut Window, cx: &mut Context<Self>) {
        self.select(self.list.row() + 1, cx);
    }

    /// Selects row `row`. A Variant row shows that Variant in the Document at once,
    /// as one undo step; the last row changes nothing.
    fn select(&mut self, row: usize, cx: &mut Context<Self>) {
        let mut editor = EditorChanges {
            editor: &self.editor,
            cx,
        };
        let outcome = self.list.select(&mut editor, row);
        self.apply(outcome, cx);
    }

    /// Esc closes the panel. Esc while the AI works stops it (results already added stay);
    /// Esc while editing drops the edit.
    fn close(&mut self, _: &ClosePanel, _: &mut Window, cx: &mut Context<Self>) {
        if self.ai_busy() {
            self.reveal = None;
            self.ai.cancel(cx);
            self.list.cancel_quick_fill();
            return;
        }
        if self.list.escape() {
            cx.emit(PanelEvent::Close);
        } else {
            cx.notify();
        }
    }

    fn delete_backward(&mut self, _: &DeleteBackward, _: &mut Window, cx: &mut Context<Self>) {
        if self.ai_busy() {
            return;
        }
        let outcome = self.list.delete_backward();
        self.apply(outcome, cx);
    }

    /// Deletes the Variant in the selected row (the Original too) with an explosion on its
    /// row.
    fn delete_variant(&mut self, _: &DeleteVariant, _: &mut Window, cx: &mut Context<Self>) {
        let row = self.list.row();
        let mut editor = EditorChanges {
            editor: &self.editor,
            cx,
        };
        let Some(range) = self.list.delete_variant(&mut editor) else {
            return;
        };
        self.explosion_row = row;
        self.explosions.play(Effect::Explosion, range, cx);
        self.apply(ListOutcome::default(), cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, Editor::undo_step);
        self.after_undo_step(cx);
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, Editor::redo_step);
        self.after_undo_step(cx);
    }

    fn after_undo_step(&mut self, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        self.list
            .after_undo_step(editor.document(), editor.target());
        self.apply(ListOutcome::default(), cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.type_text(&text, cx);
    }

    /// Typed text goes into the draft of the last row.
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let Some(typed) = keystroke.key_char.as_ref() else {
            return;
        };
        if !keystroke.modifiers.is_subset_of(&Modifiers::shift()) {
            return;
        }
        self.type_text(typed, cx);
        cx.stop_propagation();
    }

    /// The list scrolls so the selected row shows. The failed-AI row sits between the
    /// Variant rows and the last row.
    fn scroll_to_row(&self, cx: &App) {
        let failed = matches!(self.ai.status(), AiStatus::Failed(_));
        let row = self.list.row();
        let child = if row < self.rows(cx).len() {
            row
        } else {
            row + usize::from(failed)
        };
        self.scroll.scroll_to_item(child);
    }

    fn type_text(&mut self, typed: &str, cx: &mut Context<Self>) {
        if self.ai_busy() {
            return;
        }
        let outcome = self.list.type_text(self.editor.read(cx).document(), typed);
        self.apply(outcome, cx);
    }
}

impl EventEmitter<PanelEvent> for AlternativesPanel {}

/// What the panel asks of the Workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelEvent {
    Close,
}

impl Focusable for AlternativesPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl AlternativesPanel {
    /// A click on a Level tab shows the word, sentence, or paragraph around the target.
    fn choose_level(&mut self, level: Level, cx: &mut Context<Self>) {
        let outcome = self
            .list
            .choose_level(self.editor.read(cx).document(), level);
        self.apply(outcome, cx);
    }

    fn render_tabs(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.level(cx);
        let tabs = [
            (Level::Word, "Word", "alternatives-tab-word"),
            (Level::Sentence, "Sentence", "alternatives-tab-sentence"),
            (Level::Paragraph, "Paragraph", "alternatives-tab-paragraph"),
        ];
        div()
            .flex()
            .gap(px(10.))
            .italic()
            .children(tabs.into_iter().map(|(level, label, id)| {
                let color = if current == Some(level) {
                    theme.accent
                } else {
                    theme.text_muted
                };
                div()
                    .id(id)
                    .debug_selector(|| id.to_owned())
                    .cursor_pointer()
                    .text_color(color)
                    .on_click(cx.listener(move |panel, _, _, cx| panel.choose_level(level, cx)))
                    .child(label)
            }))
    }

    /// The explosion over row `index` while it plays. It only paints, so it never
    /// blocks input.
    fn explosion_at(&self, index: usize, theme: &Theme, cx: &App) -> Option<impl IntoElement> {
        if self.exploding_row(cx) != Some(index) {
            return None;
        }
        let playing: Vec<_> = self
            .explosions
            .frames(cx.background_executor().now())
            .into_iter()
            .map(|(particles, _)| particles)
            .collect();
        let background = theme.panel_background;
        let paint = move |bounds, (), window: &mut Window, cx: &mut App| {
            let now = cx.background_executor().now();
            for particles in &playing {
                if let Some(progress) = particles.progress(now) {
                    particles.paint(&[bounds], px(23.), progress, background, window);
                    window.request_animation_frame();
                }
            }
        };
        Some(
            canvas(|_, _, _| (), paint)
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        )
    }

    fn render_rows(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows(cx);
        let last = rows.len();
        let selected = self.list.row();
        let draft = self.list.draft();
        let variant_rows = rows.into_iter().enumerate().map(|(index, row)| {
            let editing = self.list.editing().filter(|_| index == selected);
            let color = if row.active {
                theme.text
            } else {
                theme.text_muted
            };
            let marker_color = if row.active && draft.is_empty() {
                theme.accent
            } else {
                theme.text_muted
            };
            let text = editing.map_or(row.text, |text| format!("{text}\u{258f}"));
            row_box(theme, index == selected)
                .children(self.explosion_at(index, theme, cx))
                .id(("alternatives-row", index))
                .debug_selector(move || format!("alternatives-row-{index}"))
                .cursor_pointer()
                .on_click(cx.listener(move |panel, _, _, cx| panel.select(index, cx)))
                .child(provenance_marker(row.provenance, marker_color))
                .child(
                    row_text()
                        .debug_selector(move || format!("alternatives-row-text-{index}"))
                        .text_color(color)
                        .child(text),
                )
        });
        let has_target = *self.list.target() != ListTarget::Nothing;
        let draft_marker = if draft.is_empty() {
            theme.text_faint
        } else {
            theme.accent
        };
        let new_row = row_box(theme, selected == last)
            .children(self.explosion_at(last, theme, cx))
            .id(("alternatives-row", last))
            .debug_selector(move || format!("alternatives-row-{last}"))
            .on_click(cx.listener(move |panel, _, _, cx| panel.select(last, cx)))
            .child(provenance_marker(Provenance::Writer, draft_marker))
            .child(
                row_text()
                    .text_color(theme.text)
                    .child(format!("{draft}\u{258f}")),
            );
        div()
            .id("alternatives-rows")
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .mt(px(35.))
            .mb(px(48.))
            .flex()
            .flex_col()
            .children(variant_rows)
            .children(self.render_ai_status(theme))
            .when(self.ai_busy(), |list| {
                let reduced_motion = settings::is_on(cx, |settings| settings.reduced_motion);
                list.child(
                    row_box(theme, true)
                        .id("alternatives-loading")
                        .debug_selector(|| "alternatives-loading".to_owned())
                        .child(provenance_marker(Provenance::Writer, theme.accent))
                        .child(loading::dance(
                            "alternatives-loading-dance",
                            theme.accent,
                            reduced_motion,
                        )),
                )
            })
            .when(
                has_target && !self.ai_busy() && !self.list.new_row_hidden(),
                |list| list.child(new_row),
            )
            .when(!has_target, |list| {
                list.child(
                    div()
                        .text_color(theme.text_muted)
                        .child("Select text, or click in a word."),
                )
            })
    }
}

impl AlternativesPanel {
    /// A Bot point row with the reason the last AI request failed. While the AI works, the
    /// empty row dances instead.
    fn render_ai_status(&self, theme: &Theme) -> Option<gpui::Stateful<gpui::Div>> {
        let row = row_box(theme, false)
            .id("alternatives-ai-status")
            .debug_selector(|| "alternatives-ai-status".to_owned());
        match self.ai.status() {
            AiStatus::Idle | AiStatus::Running | AiStatus::Done => None,
            AiStatus::Failed(reason) => Some(
                row.child(provenance_marker(Provenance::Ai, theme.accent))
                    .child(
                        row_text()
                            .text_color(theme.text)
                            .child(format!("No AI alternatives: {reason}")),
                    ),
            ),
        }
    }
}

/// A list row: about 23 px apart; the selected row has the hover background.
fn row_box(theme: &Theme, selected: bool) -> gpui::Div {
    selectable_row(theme, selected)
        .relative()
        .flex_none()
        .flex()
        .items_start()
        .gap(px(8.))
        .min_h(px(23.))
        .py(px(3.))
        .px(px(4.))
}

/// Row text: it takes the width left of the marker and wraps in it.
fn row_text() -> gpui::Div {
    div().flex_1().min_w_0()
}

/// Bullet point (Writer), Bot point (AI), or the Mixed marker: a Bot point outline
/// with a Bullet point in it.
fn provenance_marker(provenance: Provenance, color: gpui::Hsla) -> gpui::Div {
    let marker = div()
        .mt(px(6.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    match provenance {
        Provenance::Writer => marker.size(px(5.)).rounded_full().bg(color),
        Provenance::Ai => marker.size(px(7.)).rounded(px(1.5)).bg(color),
        Provenance::Mixed => marker
            .size(px(7.))
            .rounded(px(1.5))
            .border_1()
            .border_color(color)
            .child(div().size(px(3.)).rounded_full().bg(color)),
    }
}

impl Render for AlternativesPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        div()
            .id("alternatives-panel")
            .debug_selector(|| "alternatives-panel".to_owned())
            .key_context("AlternativesPanel")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::delete_backward))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::delete_variant))
            .on_key_down(cx.listener(Self::on_key_down))
            .occlude()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(theme.panel_width)
            .flex()
            .flex_col()
            .pt(px(56.))
            .px(px(16.))
            .bg(theme.panel_background)
            .border_r_1()
            .border_color(theme.panel_border)
            .text_size(px(LIST_FONT_SIZE))
            .child(self.render_tabs(&theme, cx))
            .child(self.render_rows(&theme, cx))
            .child(
                div()
                    .debug_selector(|| "alternatives-hint".to_owned())
                    .absolute()
                    .bottom(px(24.))
                    .left(px(16.))
                    .text_size(theme.font_size_hint)
                    .text_color(theme.text_muted)
                    .child("return adds \u{b7} esc closes"),
            )
    }
}
