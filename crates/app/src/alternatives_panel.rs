//! The Alternatives panel: the Variants of the Slot at the editor target, the empty row
//! that takes a new Variant, quick fill, and AI alternatives.

use std::fmt;
use std::ops::Range;
use std::time::Duration;
use std::time::Instant;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    Modifiers, Render, ScrollHandle, Subscription, Task, Window, canvas, div, prelude::*, px,
};
use ka_teb_core::{
    AiRequest, AiTrigger, ApplyError, Effect, Level, Provenance, RangeError, SlotId, Switch,
    follow_change, paragraph_around, sentence_around,
};

use crate::actions::{
    ClosePanel, ConfirmVariant, DeleteBackward, DeleteVariant, Paste, Redo, SelectNextVariant,
    SelectPreviousVariant, Undo,
};
use crate::ai::{AiRun, AiStatus};
use crate::editor::Spotlight;
use crate::effects::EffectPlayer;
use crate::sounds;
use crate::{Editor, Theme};
use crate::{context_menu, loading, settings};

/// Alternatives list text size.
const LIST_FONT_SIZE: f32 = 12.;

/// The empty-row text that starts a quick fill.
const QUICK_FILL: &str = "??";

/// Quick fill results show this many at a time (the demo shows 2, then 4, then 5).
const QUICK_FILL_BATCH: usize = 2;

/// Time between two quick fill batches.
const QUICK_FILL_BATCH_GAP: Duration = Duration::from_millis(150);

/// The batch reveal of quick fill results. Dropping it stops the reveal.
struct Reveal {
    _work: Task<()>,
    /// Added AI Variants at the end of the list not shown yet.
    hidden: usize,
}

/// One Variant row of the Alternatives panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRow {
    pub text: String,
    pub provenance: Provenance,
    /// The Active variant: bright, with an accent marker.
    pub active: bool,
}

/// Why the panel asked the AI nothing, or core refused its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiAlternativesError {
    /// The panel shows no Slot and no text.
    NoTarget,
    Range(RangeError),
    Apply(ApplyError),
}

impl fmt::Display for AiAlternativesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTarget => f.write_str("select text first"),
            Self::Range(error) => error.fmt(f),
            Self::Apply(error) => error.fmt(f),
        }
    }
}

fn ai_run(panel: &mut AlternativesPanel, (): ()) -> &mut AiRun<(), AiAlternativesError> {
    &mut panel.ai
}

/// What the panel shows: a Slot, or text with no Slot yet (its text is the Original).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Slot(SlotId),
    Text { range: Range<usize>, level: Level },
    Nothing,
}

/// The left Alternatives panel. It changes the Document only through
/// the [`Editor`], so each change is one editor undo step.
pub struct AlternativesPanel {
    editor: Entity<Editor>,
    focus_handle: FocusHandle,
    target: Target,
    /// The selected row: a Variant, or `rows().len()` for the last row, which takes a new one.
    row: usize,
    /// The text typed in the last row.
    draft: String,
    /// The text of the Variant in the selected row while the writer edits it.
    editing: Option<String>,
    /// The row that the last delete blew up.
    explosion_row: usize,
    explosions: EffectPlayer,
    /// The AI request of the context menu or of a quick fill; it stops when the panel closes.
    ai: AiRun<(), AiAlternativesError>,
    reveal: Option<Reveal>,
    /// The last row is hidden after a quick fill, until the writer goes to it.
    new_row_hidden: bool,
    scroll: ScrollHandle,
    /// The Document text at the last editor change, to move a text target along with edits.
    seen_text: String,
    _editor_changes: Subscription,
}

impl AlternativesPanel {
    pub(crate) fn new(editor: Entity<Editor>, cx: &mut Context<Self>) -> Self {
        let editor_changes = cx.observe(&editor, |panel, _, cx| {
            panel.follow_document(cx);
            cx.notify();
        });
        let seen_text = editor.read(cx).document().text().to_owned();
        let mut panel = Self {
            editor,
            focus_handle: cx.focus_handle(),
            target: Target::Nothing,
            row: 0,
            draft: String::new(),
            editing: None,
            explosion_row: 0,
            explosions: EffectPlayer::default(),
            ai: AiRun::default(),
            reveal: None,
            new_row_hidden: false,
            scroll: ScrollHandle::new(),
            seen_text,
            _editor_changes: editor_changes,
        };
        let target = panel.target_at_editor(cx);
        panel.set_target(target, cx);
        panel.row = panel.rows(cx).len();
        panel
    }

    /// The Slot that holds the editor target (innermost first), else the target text.
    fn target_at_editor(&self, cx: &App) -> Target {
        let editor = self.editor.read(cx);
        let range = editor.target();
        let document = editor.document();
        if let Some(id) = document.slot_over(&range) {
            return Target::Slot(id);
        }
        if range.is_empty() {
            return Target::Nothing;
        }
        let level = document.level_for(&range);
        Target::Text { range, level }
    }

    /// A text target moves along with a Document edit. Text edited away leaves no target.
    fn follow_document(&mut self, cx: &mut App) {
        let text = self.editor.read(cx).document().text();
        if text == self.seen_text {
            return;
        }
        let new = text.to_owned();
        let old = std::mem::replace(&mut self.seen_text, new);
        let Target::Text { range, level } = &self.target else {
            return;
        };
        let range = follow_change(range.start, &old, &self.seen_text)
            ..follow_change(range.end, &old, &self.seen_text);
        let target = if range.is_empty() {
            Target::Nothing
        } else {
            Target::Text {
                range,
                level: *level,
            }
        };
        self.set_target(target, cx);
        self.sync_preview(cx);
    }

    /// Shows `target`; its paragraph stays bright in the editor.
    fn set_target(&mut self, target: Target, cx: &mut App) {
        let spotlight = match &target {
            Target::Slot(id) => Some(Spotlight::Slot(*id)),
            Target::Text { range, .. } => Some(Spotlight::Text(range.clone())),
            Target::Nothing => None,
        };
        self.target = target;
        self.editor
            .read(cx)
            .document()
            .text()
            .clone_into(&mut self.seen_text);
        self.editor
            .update(cx, |editor, cx| editor.set_spotlight(spotlight, cx));
    }

    /// The draft of the last row shows in the Document in place of the panel target,
    /// while it has text.
    fn sync_preview(&self, cx: &mut App) {
        let range = match &self.target {
            Target::Slot(id) => self
                .editor
                .read(cx)
                .document()
                .slot(*id)
                .map(|slot| slot.range),
            Target::Text { range, .. } => Some(range.clone()),
            Target::Nothing => None,
        };
        let range = range.filter(|_| !self.draft.is_empty() && self.draft != QUICK_FILL);
        let draft = self.draft.clone();
        self.editor.update(cx, |editor, cx| {
            editor.set_preview(range.map(|range| (range, draft.as_str())), cx);
        });
    }

    /// Shows Slot `id`, with the last row selected.
    pub(crate) fn show_slot(&mut self, id: SlotId, cx: &mut Context<Self>) {
        self.set_target(Target::Slot(id), cx);
        self.editing = None;
        self.draft.clear();
        self.sync_preview(cx);
        self.row = self.rows(cx).len();
        cx.notify();
    }

    /// The Level tab that is active. `None` with nothing to show.
    #[must_use]
    pub fn level(&self, cx: &App) -> Option<Level> {
        match &self.target {
            Target::Slot(id) => self
                .editor
                .read(cx)
                .document()
                .slot(*id)
                .map(|slot| slot.level),
            Target::Text { level, .. } => Some(*level),
            Target::Nothing => None,
        }
    }

    /// The row where a delete explosion plays now. The next row has moved up into it.
    #[must_use]
    pub fn exploding_row(&self) -> Option<usize> {
        (!self.explosions.playing().is_empty()).then_some(self.explosion_row)
    }

    /// The Variants in order, the Original first. Quick fill results not shown yet
    /// are left out.
    #[must_use]
    pub fn rows(&self, cx: &App) -> Vec<VariantRow> {
        let mut rows = self.all_rows(cx);
        let hidden = self.reveal.as_ref().map_or(0, |reveal| reveal.hidden);
        rows.truncate(rows.len().saturating_sub(hidden));
        rows
    }

    fn all_rows(&self, cx: &App) -> Vec<VariantRow> {
        let document = self.editor.read(cx).document();
        match &self.target {
            Target::Slot(id) => document.slot(*id).map_or_default(|slot| {
                slot.variants
                    .iter()
                    .enumerate()
                    .map(|(index, variant)| VariantRow {
                        text: variant.text().to_owned(),
                        provenance: variant.provenance(),
                        active: index == slot.active,
                    })
                    .collect()
            }),
            Target::Text { range, .. } => document
                .text()
                .get(range.clone())
                .map(|text| VariantRow {
                    text: text.to_owned(),
                    provenance: Provenance::Writer,
                    active: true,
                })
                .into_iter()
                .collect(),
            Target::Nothing => Vec::new(),
        }
    }
}

impl AlternativesPanel {
    /// Where the AI request of the panel is. A quick fill runs until its last batch shows.
    #[must_use]
    pub fn ai_status(&self) -> AiStatus<AiAlternativesError> {
        if self.reveal.is_some() {
            return AiStatus::Running;
        }
        self.ai.status().clone()
    }

    /// The AI works, or quick fill results still come in.
    fn ai_busy(&self) -> bool {
        self.ai.is_running() || self.reveal.is_some()
    }

    /// Asks the AI for Variants of the panel target on the background
    /// executor. A request that runs already is not started again.
    pub(crate) fn request_ai(&mut self, cx: &mut Context<Self>) {
        if !self.ai_busy() {
            self.ask_ai(AiTrigger::ContextMenu, cx);
        }
    }

    fn ask_ai(&mut self, trigger: AiTrigger, cx: &mut Context<Self>) {
        let request = self.ai_request(trigger, cx);
        self.ai.start(
            (),
            request,
            |ai, request, cancel| ai.variants(request, cancel),
            ai_run,
            move |panel, request, texts, cx| panel.ai_answered(trigger, request, &texts, cx),
            cx,
        );
    }

    /// The request for the Slot or text the panel shows.
    fn ai_request(&self, trigger: AiTrigger, cx: &App) -> Result<AiRequest, AiAlternativesError> {
        let document = self.editor.read(cx).document();
        let place = match &self.target {
            Target::Slot(id) => document.slot(*id).map(|slot| (slot.range, slot.level)),
            Target::Text { range, level } => Some((range.clone(), *level)),
            Target::Nothing => None,
        };
        let (range, level) = place.ok_or(AiAlternativesError::NoTarget)?;
        document
            .ai_request(range, level, trigger)
            .map_err(AiAlternativesError::Range)
    }

    /// "??" in the last row asks the AI for Variants of the panel target.
    /// The row dances; the preview stops and "??" never becomes a Variant.
    fn start_quick_fill(&mut self, cx: &mut Context<Self>) {
        if self.ai_busy() {
            return;
        }
        self.draft.clear();
        self.sync_preview(cx);
        self.ask_ai(AiTrigger::QuickFill, cx);
    }

    /// Adds the AI texts as AI Variants in one undo step; the Active variant stays.
    /// A quick fill then shows them in batches under the Variants; a context-menu request
    /// shows the Slot that got them.
    fn ai_answered(
        &mut self,
        trigger: AiTrigger,
        request: &AiRequest,
        texts: &[String],
        cx: &mut Context<Self>,
    ) -> Result<(), AiAlternativesError> {
        let at_last_row = self.row >= self.rows(cx).len();
        let before = self.all_rows(cx).len();
        let id = self
            .editor
            .update(cx, |editor, cx| {
                editor.change_document(|document| document.apply_ai_result(request, texts), cx)
            })
            .map_err(AiAlternativesError::Apply)?;
        match trigger {
            AiTrigger::QuickFill => self.reveal(id, before, cx),
            AiTrigger::ContextMenu => self.show_answer(id, request, at_last_row, cx),
        }
        Ok(())
    }

    /// Shows the Variants a quick fill added in batches; the list had `before` rows.
    fn reveal(&mut self, id: SlotId, before: usize, cx: &mut Context<Self>) {
        if matches!(self.target, Target::Text { .. }) {
            self.set_target(Target::Slot(id), cx);
        }
        let hidden = self.all_rows(cx).len().saturating_sub(before.max(1));
        let work = cx.spawn(async move |this, cx| {
            loop {
                let more = this.update(cx, AlternativesPanel::reveal_batch);
                if !matches!(more, Ok(true)) {
                    return;
                }
                cx.background_executor().timer(QUICK_FILL_BATCH_GAP).await;
            }
        });
        self.reveal = Some(Reveal {
            _work: work,
            hidden,
        });
        cx.notify();
    }

    /// Shows the next batch of quick fill results. `false` when none are left: the quick
    /// fill is done.
    fn reveal_batch(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(reveal) = self.reveal.as_mut() else {
            return false;
        };
        reveal.hidden = reveal.hidden.saturating_sub(QUICK_FILL_BATCH);
        cx.notify();
        if reveal.hidden > 0 {
            return true;
        }
        self.end_quick_fill(cx);
        false
    }

    /// The "??" row goes; the active Variant row is selected and the last row hides.
    fn end_quick_fill(&mut self, cx: &mut Context<Self>) {
        self.reveal = None;
        self.new_row_hidden = true;
        let active = self.rows(cx).iter().position(|row| row.active);
        self.row = active.unwrap_or_else(|| self.rows(cx).len());
        cx.notify();
    }

    /// Esc while the row dances stops the AI request. Results already added stay.
    fn cancel_ai(&mut self, cx: &mut Context<Self>) {
        self.reveal = None;
        self.ai.cancel(cx);
        self.draft.clear();
    }

    /// The panel shows Slot `id` when it showed the text of `request`.
    fn show_answer(
        &mut self,
        id: SlotId,
        request: &AiRequest,
        at_last_row: bool,
        cx: &mut Context<Self>,
    ) {
        let showed_request = match &self.target {
            Target::Text { range, .. } => *range == request.range(),
            Target::Slot(shown) => *shown == id,
            Target::Nothing => false,
        };
        if showed_request {
            self.set_target(Target::Slot(id), cx);
        }
        if at_last_row {
            self.row = self.rows(cx).len();
        }
    }
}

impl AlternativesPanel {
    /// Return saves the edit in progress, starts an edit on a Variant row,
    /// or adds the draft of the last row as a Variant.
    fn confirm(&mut self, _: &ConfirmVariant, _: &mut Window, cx: &mut Context<Self>) {
        if self.ai_busy() {
            return;
        }
        if let Some(text) = self.editing.take() {
            self.save_edit(&text, cx);
            return;
        }
        let rows = self.rows(cx);
        let Some(row) = rows.get(self.row) else {
            self.add(cx);
            return;
        };
        if matches!(self.target, Target::Slot(_)) {
            self.editing = Some(row.text.clone());
            cx.notify();
        }
    }

    /// Sets the text of the Variant in the selected row, as a writer edit.
    fn save_edit(&mut self, text: &str, cx: &mut Context<Self>) {
        cx.notify();
        let Target::Slot(id) = self.target else {
            return;
        };
        let row = self.row;
        self.editor
            .update(cx, |editor, cx| {
                editor.change_document(|document| document.edit_variant(id, row, text), cx)
            })
            .ok();
    }

    /// Adds the draft as a Variant and makes it the Active variant, as one undo
    /// step. With no Slot yet, it makes one over the target text, the Original first.
    fn add(&mut self, cx: &mut Context<Self>) {
        if self.draft.is_empty() || self.draft == QUICK_FILL {
            return;
        }
        let draft = self.draft.clone();
        let switched = match self.target.clone() {
            Target::Slot(id) => self
                .editor
                .update(cx, |editor, cx| {
                    editor.change_document(
                        |document| document.add_active_variant(id, &draft, Provenance::Writer),
                        cx,
                    )
                })
                .ok(),
            Target::Text { range, level } => {
                let created = self.editor.update(cx, |editor, cx| {
                    editor.change_document(
                        |document| {
                            document.create_active_slot(range, level, &draft, Provenance::Writer)
                        },
                        cx,
                    )
                });
                created
                    .map(|id| {
                        self.set_target(Target::Slot(id), cx);
                        Switch::ToVariant
                    })
                    .ok()
            }
            Target::Nothing => None,
        };
        if let Some(switch) = switched {
            sounds::play(switch.sound(), cx);
            self.draft.clear();
            self.sync_preview(cx);
            self.row = self.rows(cx).len();
            cx.notify();
        }
    }

    fn select_previous(
        &mut self,
        _: &SelectPreviousVariant,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select(self.row.saturating_sub(1), cx);
    }

    fn select_next(&mut self, _: &SelectNextVariant, _: &mut Window, cx: &mut Context<Self>) {
        self.select(self.row + 1, cx);
    }

    /// Selects row `row`. A Variant row shows that Variant in the Document at once,
    /// as one undo step; the last row changes nothing.
    fn select(&mut self, row: usize, cx: &mut Context<Self>) {
        let last = self.rows(cx).len();
        let row = row.min(last);
        if row == last {
            self.new_row_hidden = false;
        }
        self.row = row;
        self.editing = None;
        self.scroll_to_row(cx);
        cx.notify();
        let Target::Slot(id) = self.target else {
            return;
        };
        let active = self
            .editor
            .read(cx)
            .document()
            .slot(id)
            .map(|slot| slot.active);
        if active.is_none_or(|active| active == row) {
            return;
        }
        let switched = self.editor.update(cx, |editor, cx| {
            editor.change_document(|document| document.set_active(id, row), cx)
        });
        if let Ok(switch) = switched {
            sounds::play(switch.sound(), cx);
        }
    }

    /// Esc closes the panel. Esc while the AI works stops it; Esc while editing drops the
    /// edit.
    fn close(&mut self, _: &ClosePanel, _: &mut Window, cx: &mut Context<Self>) {
        if self.ai_busy() {
            self.cancel_ai(cx);
            return;
        }
        if self.editing.take().is_some() {
            cx.notify();
            return;
        }
        cx.emit(PanelEvent::Close);
    }

    fn delete_backward(&mut self, _: &DeleteBackward, _: &mut Window, cx: &mut Context<Self>) {
        if self.ai_busy() {
            return;
        }
        if let Some(text) = self.editing.as_mut() {
            text.pop();
        } else {
            self.draft.pop();
            self.sync_preview(cx);
        }
        cx.notify();
    }

    /// Deletes the Variant in the selected row (the Original too) with an explosion on its
    /// row. A Slot left with one Variant ends; the panel then shows its text with no Slot.
    fn delete_variant(&mut self, _: &DeleteVariant, _: &mut Window, cx: &mut Context<Self>) {
        let Target::Slot(id) = self.target else {
            return;
        };
        let Some(slot) = self.editor.read(cx).document().slot(id) else {
            return;
        };
        let row = self.row;
        if row >= slot.variants.len() {
            return;
        }
        let level = slot.level;
        let range = slot.range.clone();
        let deleted = self
            .editor
            .update(cx, |editor, cx| editor.delete_variant(id, row, cx));
        let Ok(kept) = deleted else {
            return;
        };
        self.explosion_row = row;
        self.explosions.play(Effect::Explosion, range, cx);
        if let Some(range) = kept {
            self.set_target(Target::Text { range, level }, cx);
        }
        self.editing = None;
        self.row = self.rows(cx).len();
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, Editor::undo_step);
        self.after_history_step(cx);
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, Editor::redo_step);
        self.after_history_step(cx);
    }

    /// After an undo or redo the edit in progress ends. A Slot that the step removed gives
    /// way to the target at the editor selection.
    fn after_history_step(&mut self, cx: &mut Context<Self>) {
        self.editing = None;
        if let Target::Slot(id) = self.target
            && self.editor.read(cx).document().slot(id).is_none()
        {
            let target = self.target_at_editor(cx);
            self.set_target(target, cx);
        }
        self.row = self.row.min(self.rows(cx).len());
        cx.notify();
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
        let child = if self.row < self.rows(cx).len() {
            self.row
        } else {
            self.row + usize::from(failed)
        };
        self.scroll.scroll_to_item(child);
    }

    /// Typed text goes into the edit in progress, else into the last row, which it selects.
    fn type_text(&mut self, typed: &str, cx: &mut Context<Self>) {
        if self.ai_busy() {
            return;
        }
        let typed = typed.replace('\n', " ");
        cx.notify();
        if let Some(text) = self.editing.as_mut() {
            text.push_str(&typed);
            return;
        }
        self.row = self.rows(cx).len();
        self.new_row_hidden = false;
        self.scroll_to_row(cx);
        self.draft.push_str(&typed);
        if self.draft == QUICK_FILL {
            self.start_quick_fill(cx);
            return;
        }
        self.sync_preview(cx);
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
        let document = self.editor.read(cx).document();
        let anchor = match &self.target {
            Target::Slot(id) => document.slot(*id).map(|slot| slot.range),
            Target::Text { range, .. } => Some(range.clone()),
            Target::Nothing => None,
        };
        let Some(anchor) = anchor else {
            return;
        };
        let text = document.text();
        let range = match level {
            Level::Word => context_menu::word_at(text, anchor.start).unwrap_or(anchor),
            Level::Sentence => sentence_around(text, &anchor),
            Level::Paragraph => paragraph_around(text, &anchor),
        };
        let slot = document
            .slots()
            .into_iter()
            .rev()
            .find(|slot| slot.range == range && slot.level == level)
            .map(|slot| slot.id);
        let target = slot.map_or(Target::Text { range, level }, Target::Slot);
        self.set_target(target, cx);
        self.sync_preview(cx);
        self.editing = None;
        self.row = self.rows(cx).len();
        cx.notify();
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
    fn explosion_at(&self, index: usize, theme: &Theme) -> Option<impl IntoElement> {
        if self.exploding_row() != Some(index) {
            return None;
        }
        let playing: Vec<_> = self
            .explosions
            .frames(Instant::now())
            .into_iter()
            .map(|(particles, _)| particles)
            .collect();
        let background = theme.panel_background;
        let paint = move |bounds, (), window: &mut Window, _: &mut App| {
            let now = Instant::now();
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
        let variant_rows = rows.into_iter().enumerate().map(|(index, row)| {
            let editing = self.editing.as_ref().filter(|_| index == self.row);
            let color = if row.active {
                theme.text
            } else {
                theme.text_muted
            };
            let marker_color = if row.active && self.draft.is_empty() {
                theme.accent
            } else {
                theme.text_muted
            };
            let text = editing.map_or(row.text, |text| format!("{text}\u{258f}"));
            row_box(theme, index == self.row)
                .children(self.explosion_at(index, theme))
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
        let has_target = self.target != Target::Nothing;
        let draft_marker = if self.draft.is_empty() {
            theme.text_faint
        } else {
            theme.accent
        };
        let new_row = row_box(theme, self.row == last)
            .children(self.explosion_at(last, theme))
            .id(("alternatives-row", last))
            .debug_selector(move || format!("alternatives-row-{last}"))
            .on_click(cx.listener(move |panel, _, _, cx| panel.select(last, cx)))
            .child(provenance_marker(Provenance::Writer, draft_marker))
            .child(
                row_text()
                    .text_color(theme.text)
                    .child(format!("{}\u{258f}", self.draft)),
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
                has_target && !self.ai_busy() && !self.new_row_hidden,
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
    div()
        .relative()
        .flex_none()
        .flex()
        .items_start()
        .gap(px(8.))
        .min_h(px(23.))
        .py(px(3.))
        .px(px(4.))
        .rounded(theme.elevated_radius)
        .when(selected, |row| row.bg(theme.elevated_row_hover))
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
