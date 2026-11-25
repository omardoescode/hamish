use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    Modifiers, Render, Subscription, Task, Window, canvas, div, prelude::*, px,
};
use ka_teb_core::{
    AiError, AiRequest, AiTrigger, Effect, Level, Provenance, SlotId, paragraph_around,
    sentence_around,
};

use crate::actions::{
    ClosePanel, ConfirmVariant, DeleteBackward, DeleteVariant, Paste, Redo, SelectNextVariant,
    SelectPreviousVariant, Undo,
};
use crate::editor::Spotlight;
use crate::effects::{self, Particles, PlayingEffect};
use crate::sounds;
use crate::{Ai, Editor, Theme};
use crate::{context_menu, loading, settings_modal};

/// Alternatives list text (spec: about 12 px).
const LIST_FONT_SIZE: f32 = 12.;

/// The empty-row text that starts a quick fill (FR-69).
const QUICK_FILL: &str = "??";

/// Quick fill results show this many at a time (FR-72: 2, then 4, then 5 in the demo).
const QUICK_FILL_BATCH: usize = 2;

/// Time between two quick fill batches.
const QUICK_FILL_BATCH_GAP: Duration = Duration::from_millis(150);

/// A quick fill in progress (FR-69 to FR-73). Dropping it cancels its work.
struct QuickFill {
    /// The AI request, then the batch reveal.
    _work: Task<()>,
    /// Added AI Variants at the end of the list not shown yet.
    hidden: usize,
}

/// One Variant row of the Alternatives panel (FR-22, FR-23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRow {
    pub text: String,
    pub provenance: Provenance,
    /// The Active variant: bright, with an accent marker.
    pub active: bool,
}

/// Where the AI request of the panel is (FR-34).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiStatus {
    Idle,
    /// The AI works on a request.
    Loading,
    /// The last request added no Variant; the reason.
    Failed(String),
}

/// What the panel shows: a Slot, or text with no Slot yet (its text is the Original).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Slot(SlotId),
    Text { range: Range<usize>, level: Level },
    Nothing,
}

/// The left Alternatives panel (FR-19 to FR-27). It changes the Document only through
/// the [`Editor`], so each change is one editor undo step.
pub struct AlternativesPanel {
    editor: Entity<Editor>,
    focus_handle: FocusHandle,
    target: Target,
    /// The selected row: a Variant, or `rows().len()` for the last row, which takes a new one.
    row: usize,
    /// The text typed in the last row.
    draft: String,
    /// The text of the selected Variant while the writer edits it (FR-26).
    editing: Option<String>,
    /// FR-80: the row that a delete blows up, and its explosion.
    explosion: Option<(usize, Particles)>,
    ai: AiStatus,
    quick_fill: Option<QuickFill>,
    /// The last row is hidden after a quick fill, until the writer goes to it (FR-73).
    new_row_hidden: bool,
    _editor_changes: Subscription,
}

impl AlternativesPanel {
    pub(crate) fn new(editor: Entity<Editor>, cx: &mut Context<Self>) -> Self {
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        let mut panel = Self {
            editor,
            focus_handle: cx.focus_handle(),
            target: Target::Nothing,
            row: 0,
            draft: String::new(),
            editing: None,
            explosion: None,
            ai: AiStatus::Idle,
            quick_fill: None,
            new_row_hidden: false,
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
        let slot = document
            .slots()
            .into_iter()
            .rev()
            .find(|slot| slot.range.start <= range.start && range.end <= slot.range.end);
        if let Some(slot) = slot {
            return Target::Slot(slot.id);
        }
        if range.is_empty() {
            return Target::Nothing;
        }
        let level = level_for(document.text(), &range);
        Target::Text { range, level }
    }

    /// Shows `target`; its paragraph stays bright in the editor (FR-27).
    fn set_target(&mut self, target: Target, cx: &mut App) {
        let spotlight = match &target {
            Target::Slot(id) => Some(Spotlight::Slot(*id)),
            Target::Text { range, .. } => Some(Spotlight::Text(range.clone())),
            Target::Nothing => None,
        };
        self.target = target;
        self.editor
            .update(cx, |editor, cx| editor.set_spotlight(spotlight, cx));
    }

    /// FR-68: the draft of the last row shows in the Document in place of the panel target,
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

    /// FR-21: shows Slot `id`, with the last row selected.
    pub(crate) fn show_slot(&mut self, id: SlotId, cx: &mut Context<Self>) {
        self.set_target(Target::Slot(id), cx);
        self.editing = None;
        self.draft.clear();
        self.sync_preview(cx);
        self.row = self.rows(cx).len();
        cx.notify();
    }

    /// The Level tab that is active (FR-20). `None` with nothing to show.
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

    /// The row where a delete explosion plays now (FR-80). The next row has moved up into it.
    #[must_use]
    pub fn exploding_row(&self) -> Option<usize> {
        let (row, playing) = self.explosion.as_ref()?;
        playing.progress(Instant::now()).map(|_| *row)
    }

    /// The Variants in order, the Original first (FR-22). Quick fill results not shown yet
    /// are left out (FR-72).
    #[must_use]
    pub fn rows(&self, cx: &App) -> Vec<VariantRow> {
        let mut rows = self.all_rows(cx);
        let hidden = self.quick_fill.as_ref().map_or(0, |fill| fill.hidden);
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
    #[must_use]
    pub fn ai_status(&self) -> AiStatus {
        self.ai.clone()
    }

    /// FR-32, FR-34: asks the AI for Variants of the panel target on the background
    /// executor. A request that runs already is not started again.
    pub(crate) fn request_ai(&mut self, cx: &mut Context<Self>) {
        if self.ai == AiStatus::Loading {
            return;
        }
        let request = self.ai_request(AiTrigger::ContextMenu, cx);
        let ai = cx.try_global::<Ai>().map(|ai| Arc::clone(&ai.0));
        let (request, ai) = match (request, ai) {
            (Ok(request), Some(ai)) => (request, ai),
            (Err(message), _) => return self.ai_failed(message, cx),
            (Ok(_), None) => return self.ai_failed("no AI backend is set".to_owned(), cx),
        };
        self.ai = AiStatus::Loading;
        cx.notify();
        let asking = cx.background_spawn(async move {
            let answer = ai.variants(&request);
            (request, answer)
        });
        let editor = self.editor.downgrade();
        cx.spawn(async move |this, cx| {
            let (request, answer) = asking.await;
            let shown = this.update(cx, |panel, cx| {
                panel.ai_answered(&request, answer.clone(), cx);
            });
            if shown.is_ok() {
                return;
            }
            let Ok(texts) = answer else {
                return;
            };
            editor
                .update(cx, |editor, cx| {
                    editor
                        .change_document(|document| document.apply_ai_result(&request, &texts), cx)
                })
                .ok();
        })
        .detach();
    }

    /// The request for the Slot or text the panel shows.
    fn ai_request(&self, trigger: AiTrigger, cx: &App) -> Result<AiRequest, String> {
        let document = self.editor.read(cx).document();
        let place = match &self.target {
            Target::Slot(id) => document.slot(*id).map(|slot| (slot.range, slot.level)),
            Target::Text { range, level } => Some((range.clone(), *level)),
            Target::Nothing => None,
        };
        let (range, level) = place.ok_or_else(|| "select text first".to_owned())?;
        document
            .ai_request(range, level, trigger)
            .map_err(|error| error.to_string())
    }

    fn ai_failed(&mut self, message: String, cx: &mut Context<Self>) {
        self.ai = AiStatus::Failed(message);
        cx.notify();
    }

    /// FR-69, FR-70: "??" in the last row asks the AI for Variants of the panel target.
    /// The row dances (FR-71); the preview stops and "??" never becomes a Variant.
    fn start_quick_fill(&mut self, cx: &mut Context<Self>) {
        if self.ai == AiStatus::Loading {
            return;
        }
        self.sync_preview(cx);
        let request = self.ai_request(AiTrigger::QuickFill, cx);
        let ai = cx.try_global::<Ai>().map(|ai| Arc::clone(&ai.0));
        let (request, ai) = match (request, ai) {
            (Ok(request), Some(ai)) => (request, ai),
            (Err(message), _) => return self.quick_fill_failed(message, cx),
            (Ok(_), None) => {
                return self.quick_fill_failed("no AI backend is set".to_owned(), cx);
            }
        };
        self.ai = AiStatus::Loading;
        cx.notify();
        let asking = cx.background_spawn(async move {
            let answer = ai.variants(&request);
            (request, answer)
        });
        let work = cx.spawn(async move |this, cx| {
            let (request, answer) = asking.await;
            this.update(cx, |panel, cx| {
                panel.quick_fill_answered(&request, answer, cx);
            })
            .ok();
        });
        self.quick_fill = Some(QuickFill {
            _work: work,
            hidden: 0,
        });
    }

    /// FR-72: adds the results as AI Variants in one undo step, then shows them in batches
    /// under the Variants. The Active variant stays (FR-36).
    fn quick_fill_answered(
        &mut self,
        request: &AiRequest,
        answer: Result<Vec<String>, AiError>,
        cx: &mut Context<Self>,
    ) {
        let before = self.all_rows(cx).len();
        let added = answer.map_err(|error| error.to_string()).and_then(|texts| {
            self.editor
                .update(cx, |editor, cx| {
                    editor.change_document(|document| document.apply_ai_result(request, &texts), cx)
                })
                .map_err(|error| error.to_string())
        });
        let id = match added {
            Ok(id) => id,
            Err(message) => return self.quick_fill_failed(message, cx),
        };
        if matches!(self.target, Target::Text { .. }) {
            self.set_target(Target::Slot(id), cx);
        }
        let hidden = self.all_rows(cx).len().saturating_sub(before.max(1));
        let reveal = cx.spawn(async move |this, cx| {
            loop {
                let more = this.update(cx, AlternativesPanel::reveal_batch);
                if !matches!(more, Ok(true)) {
                    return;
                }
                cx.background_executor().timer(QUICK_FILL_BATCH_GAP).await;
            }
        });
        self.quick_fill = Some(QuickFill {
            _work: reveal,
            hidden,
        });
        cx.notify();
    }

    /// Shows the next batch of quick fill results. `false` when none are left: the quick
    /// fill is done.
    fn reveal_batch(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(fill) = self.quick_fill.as_mut() else {
            return false;
        };
        fill.hidden = fill.hidden.saturating_sub(QUICK_FILL_BATCH);
        cx.notify();
        if fill.hidden > 0 {
            return true;
        }
        self.end_quick_fill(cx);
        false
    }

    /// FR-73: the "??" row goes; the active Variant row is selected and the last row hides.
    fn end_quick_fill(&mut self, cx: &mut Context<Self>) {
        self.quick_fill = None;
        self.ai = AiStatus::Idle;
        self.draft.clear();
        self.new_row_hidden = true;
        let active = self.rows(cx).iter().position(|row| row.active);
        self.row = active.unwrap_or_else(|| self.rows(cx).len());
        cx.notify();
    }

    /// FR-82: the dance stops, the row is empty again, and the panel says why.
    fn quick_fill_failed(&mut self, message: String, cx: &mut Context<Self>) {
        self.quick_fill = None;
        self.draft.clear();
        self.ai_failed(message, cx);
    }

    /// FR-82: Esc while the row dances stops the quick fill. Results already added stay.
    fn cancel_quick_fill(&mut self, cx: &mut Context<Self>) {
        self.quick_fill = None;
        self.ai = AiStatus::Idle;
        self.draft.clear();
        cx.notify();
    }

    /// FR-33, FR-36: adds the AI texts as AI Variants in one undo step; the Active variant
    /// stays. The panel then shows the Slot that got them.
    fn ai_answered(
        &mut self,
        request: &AiRequest,
        answer: Result<Vec<String>, AiError>,
        cx: &mut Context<Self>,
    ) {
        let at_last_row = self.row >= self.rows(cx).len();
        let added = answer.map_err(|error| error.to_string()).and_then(|texts| {
            self.editor
                .update(cx, |editor, cx| {
                    editor.change_document(|document| document.apply_ai_result(request, &texts), cx)
                })
                .map_err(|error| error.to_string())
        });
        let id = match added {
            Ok(id) => id,
            Err(message) => return self.ai_failed(message, cx),
        };
        self.ai = AiStatus::Idle;
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
        cx.notify();
    }
}

impl AlternativesPanel {
    /// FR-24, FR-26: Return saves the edit in progress, starts an edit on a Variant row,
    /// or adds the draft of the last row as a Variant.
    fn confirm(&mut self, _: &ConfirmVariant, _: &mut Window, cx: &mut Context<Self>) {
        if self.quick_fill.is_some() {
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

    /// FR-26: sets the text of the selected Variant, as a writer edit.
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

    /// FR-24: adds the draft as a Variant and makes it the Active variant, as one undo
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
                    editor.change_document_in_steps(
                        2,
                        |document| {
                            document.add_variant(id, &draft, Provenance::Writer)?;
                            let last = document
                                .slot(id)
                                .map_or(0, |slot| slot.variants.len().saturating_sub(1));
                            document.set_active(id, last)
                        },
                        cx,
                    )
                })
                .ok(),
            Target::Text { range, level } => {
                let created = self.editor.update(cx, |editor, cx| {
                    editor.change_document_in_steps(
                        2,
                        |document| {
                            let id =
                                document.create_slot(range, level, &draft, Provenance::Writer)?;
                            document.set_active(id, 1).map(|switch| (id, switch))
                        },
                        cx,
                    )
                });
                created
                    .map(|(id, switch)| {
                        self.set_target(Target::Slot(id), cx);
                        switch
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

    /// FR-25: selects row `row`. A Variant row shows that Variant in the Document at once,
    /// as one undo step; the last row changes nothing.
    fn select(&mut self, row: usize, cx: &mut Context<Self>) {
        let last = self.rows(cx).len();
        let row = row.min(last);
        if row == last {
            self.new_row_hidden = false;
        }
        self.row = row;
        self.editing = None;
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

    /// FR-24: Esc closes the panel.
    /// Esc while editing drops the edit.
    fn close(&mut self, _: &ClosePanel, _: &mut Window, cx: &mut Context<Self>) {
        if self.quick_fill.is_some() {
            self.cancel_quick_fill(cx);
            return;
        }
        if self.editing.take().is_some() {
            cx.notify();
            return;
        }
        cx.emit(PanelEvent::Close);
    }

    fn delete_backward(&mut self, _: &DeleteBackward, _: &mut Window, cx: &mut Context<Self>) {
        if self.quick_fill.is_some() {
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

    /// FR-26, FR-80: deletes the selected Variant (the Original too) with an explosion on its
    /// row. A Slot left with one Variant ends (FR-18); the panel then shows its text with no Slot.
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
        let start = slot.range.start;
        let kept = (slot.variants.len() == 2).then(|| {
            let other = usize::from(row == 0);
            slot.variants
                .get(other)
                .map_or_default(|variant| variant.text().to_owned())
        });
        let deleted = self
            .editor
            .update(cx, |editor, cx| editor.delete_variant(id, row, cx));
        if deleted.is_err() {
            return;
        }
        let playing = PlayingEffect {
            effect: Effect::Explosion,
            range,
        };
        let seed = u64::try_from(row).unwrap_or_default().wrapping_add(0x5eed);
        self.explosion = effects::plays(cx).then(|| (row, Particles::new(playing, seed)));
        if let Some(kept) = kept {
            let text = self.editor.read(cx).document().text();
            let target = ended_slot_text(text, start, &kept)
                .map_or(Target::Nothing, |range| Target::Text { range, level });
            self.set_target(target, cx);
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

    /// Typed text goes into the edit in progress, else into the last row, which it selects.
    fn type_text(&mut self, typed: &str, cx: &mut Context<Self>) {
        if self.ai == AiStatus::Loading {
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
        self.draft.push_str(&typed);
        if self.draft == QUICK_FILL {
            self.start_quick_fill(cx);
            return;
        }
        self.sync_preview(cx);
    }
}

/// Where the text `kept` of a Slot that ended at `start` lies now. An "a"/"an" before it
/// may have changed (Article agreement), which moves it by one byte.
fn ended_slot_text(text: &str, start: usize, kept: &str) -> Option<Range<usize>> {
    [start, start + 1, start.saturating_sub(1)]
        .into_iter()
        .map(|start| start..start + kept.len())
        .find(|range| text.get(range.clone()) == Some(kept))
}

/// The Level a new Slot over `range` of `text` gets: Paragraph for a whole paragraph or
/// more, Sentence for text with sentence-end punctuation, else Word (a word or short phrase).
fn level_for(text: &str, range: &Range<usize>) -> Level {
    let selected = text.get(range.clone()).unwrap_or_default();
    let paragraph = text
        .get(paragraph_around(text, range))
        .unwrap_or_default()
        .trim();
    if selected.contains('\n') || (!paragraph.is_empty() && selected.trim() == paragraph) {
        return Level::Paragraph;
    }
    if selected.contains(['.', '!', '?']) {
        return Level::Sentence;
    }
    Level::Word
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

    /// FR-80: the explosion over row `index` while it plays. It only paints, so it never
    /// blocks input.
    fn explosion_at(&self, index: usize, theme: &Theme) -> Option<impl IntoElement> {
        let (_, playing) = self
            .explosion
            .as_ref()
            .filter(|_| self.exploding_row() == Some(index))?;
        let playing = playing.clone();
        let background = theme.panel_background;
        let paint = move |bounds, (), window: &mut Window, _: &mut App| {
            let Some(progress) = playing.progress(Instant::now()) else {
                return;
            };
            playing.paint(&[bounds], px(23.), progress, background, window);
            window.request_animation_frame();
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
                .child(div().text_color(color).child(text))
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
                div()
                    .text_color(theme.text)
                    .child(format!("{}\u{258f}", self.draft)),
            );
        div()
            .mt(px(35.))
            .flex()
            .flex_col()
            .children(variant_rows)
            .children(self.render_ai_status(theme))
            .when(self.ai == AiStatus::Loading, |list| {
                let reduced_motion = settings_modal::is_on(cx, |settings| settings.reduced_motion);
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
                has_target && self.ai != AiStatus::Loading && !self.new_row_hidden,
                |list| list.child(new_row),
            )
            .when(!has_target, |list| {
                list.child(
                    div()
                        .text_color(theme.text_muted)
                        .child("Select text, or put the cursor in a word."),
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
        match &self.ai {
            AiStatus::Idle | AiStatus::Loading => None,
            AiStatus::Failed(reason) => Some(
                row.child(provenance_marker(Provenance::Ai, theme.accent))
                    .child(
                        div()
                            .text_color(theme.text)
                            .child(format!("No AI alternatives: {reason}")),
                    ),
            ),
        }
    }
}

/// A list row: about 23 px apart (spec); the selected row has the hover background.
fn row_box(theme: &Theme, selected: bool) -> gpui::Div {
    div()
        .relative()
        .flex()
        .items_start()
        .gap(px(8.))
        .min_h(px(23.))
        .py(px(3.))
        .px(px(4.))
        .rounded(theme.elevated_radius)
        .when(selected, |row| row.bg(theme.elevated_row_hover))
}

/// FR-23: Bullet point (Writer), Bot point (AI), or the Mixed marker: a Bot point outline
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
                    .absolute()
                    .bottom(px(24.))
                    .left(px(16.))
                    .text_size(theme.font_size_hint)
                    .text_color(theme.text_muted)
                    .child("return adds \u{b7} esc closes"),
            )
    }
}

#[cfg(test)]
mod tests {
    use ka_teb_core::Level;

    use super::{ended_slot_text, level_for};

    const TEXT: &str = "Title\nThe customer waits. She leaves.\nNext one.";

    #[test]
    fn the_text_of_an_ended_slot_is_found_past_a_changed_article() {
        let text = "Title\nIt is an eraser here.";
        assert_eq!(ended_slot_text(text, 14, "eraser"), Some(15..21));
        assert_eq!(ended_slot_text(text, 15, "eraser"), Some(15..21));
        assert_eq!(ended_slot_text(text, 14, "thumbtack"), None);
    }

    #[test]
    fn a_word_or_short_phrase_is_word_level() {
        assert_eq!(level_for(TEXT, &(10..18)), Level::Word);
        assert_eq!(level_for(TEXT, &(6..18)), Level::Word);
    }

    #[test]
    fn text_with_sentence_end_punctuation_is_sentence_level() {
        assert_eq!(level_for(TEXT, &(6..25)), Level::Sentence);
    }

    #[test]
    fn a_whole_paragraph_or_text_across_lines_is_paragraph_level() {
        assert_eq!(level_for(TEXT, &(6..37)), Level::Paragraph);
        assert_eq!(level_for(TEXT, &(26..42)), Level::Paragraph);
    }
}
