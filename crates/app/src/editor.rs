//! The editor: our own GPUI text element over a core `EditSession` (ADR 0002). It owns the
//! view state: Markers, hover, scroll, and the Effects over the text.

use std::ops::Range;
use std::time::{Duration, Instant};

use crate::Theme;
use crate::actions::{
    AiAlternatives, CloseMenu, DeleteActiveVariant, OpenAlternatives, Redo, Stash, ToggleGhost,
    Undo,
};
use crate::caret::{self, CaretView, Column, caret_actions, caret_input_handler};
use crate::context_menu::{ContextMenu, ContextMenuItem};
use crate::effects::{self, EffectPlayer, PlayingEffect};
use crate::settings;
use crate::sounds;
use gpui::{
    App, Bounds, Context, CursorStyle, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render,
    ScrollHandle, SharedString, Window, div, prelude::*,
};
use hamish_core::{
    Caret, CaretEdit, Cycle, Document, EditSession, Effect, FocusZoom, LabMark, Level, SlotError,
    SlotId, Sound, TrimProposal, Vim, VimCommand, VimKey, VimMode, follow_change, paragraph_around,
};

use element::EditorElement;
use pen::PenStroke;
use runs::{Span, spans};

mod element;
mod focus_zoom;
mod painted_text;
mod pen;
mod runs;
mod synthetic_bold;

pub(crate) use painted_text::{PaintedText, shape};

/// How a Slot shows that it has Variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// Word: underline plus a row of Variant dots under the word.
    Underline,
    /// Sentence: underline under the rows of the sentence only, plus a row of Variant dots
    /// after its last char.
    SentenceUnderline,
    /// Paragraph: a line in the left margin plus a column of Variant dots.
    MarginLine,
}

/// The Marker of one visible Slot: `dots` Variant dots, dot `active` filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub slot: SlotId,
    pub kind: MarkerKind,
    pub dots: usize,
    pub active: usize,
}

/// Opacity of new Active variant text when its fade-in starts.
const FADE_IN_START_OPACITY: f32 = 0.2;

/// The text column: our own GPUI editor element over a core [`Document`] (ADR 0002).
pub struct Editor {
    session: EditSession,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    painted: Option<PaintedText>,
    selecting: bool,
    autoscroll: bool,
    hovered: Option<SlotId>,
    markers: Vec<Marker>,
    /// The Variant dots of each Marker as last painted (window coordinates).
    marker_dots: Vec<element::MarkerDot>,
    hover_tint: Vec<Bounds<Pixels>>,
    underlines: Vec<PenStroke>,
    context_menu: Option<ContextMenu>,
    spotlight: Option<Spotlight>,
    /// The Document with the draft of the Alternatives panel in place; it is painted,
    /// never saved.
    preview: Option<Document>,
    /// The Slot whose Active variant last changed, and when.
    fade_in: Option<(SlotId, Instant)>,
    /// Effects that play over the text.
    effects: EffectPlayer,
    focus_zoom: FocusZoom,
    /// The zoomed paragraphs as last painted.
    zooms: Vec<focus_zoom::Zoom>,
    column: Option<Column>,
    /// Where a dragged Overflow block goes in if it drops now.
    drop_target: Option<usize>,
    /// The font family of the text as last painted.
    text_font: SharedString,
    /// Offsets of the glyphs last painted with synthetic bold.
    synthetic_bold: Vec<usize>,
    /// The Vim key state; it starts again each time Vim mode turns on.
    vim: Vim,
}

/// What the open Alternatives panel shows: its paragraph stays bright, the rest dims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Spotlight {
    Slot(SlotId),
    Text(Range<usize>),
}

/// What the editor tells the views around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorEvent {
    /// A click on the text of a visible Slot.
    SlotClicked(SlotId),
    /// A Variant of the Slot was deleted; its explosion and its sound play.
    VariantDeleted(SlotId),
    /// An undo put a deleted Variant of the Slot back; its burst plays.
    VariantRestored(SlotId),
}

impl EventEmitter<EditorEvent> for Editor {}

impl Editor {
    pub fn new(document: Document, cx: &mut Context<Self>) -> Self {
        Self {
            session: EditSession::new(document),
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            painted: None,
            selecting: false,
            autoscroll: false,
            hovered: None,
            markers: Vec::new(),
            marker_dots: Vec::new(),
            hover_tint: Vec::new(),
            underlines: Vec::new(),
            context_menu: None,
            spotlight: None,
            preview: None,
            fade_in: None,
            effects: EffectPlayer::default(),
            focus_zoom: FocusZoom::default(),
            zooms: Vec::new(),
            column: None,
            drop_target: None,
            text_font: SharedString::default(),
            synthetic_bold: Vec::new(),
            vim: Vim::default(),
        }
    }

    /// The font family of the text as last painted.
    #[must_use]
    pub fn text_font(&self) -> &SharedString {
        &self.text_font
    }

    /// Byte offsets (in the shown Document) of the glyphs last painted with synthetic bold:
    /// bold text in a font with no bold face.
    #[must_use]
    pub fn synthetic_bold(&self) -> Vec<usize> {
        self.synthetic_bold.clone()
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        self.session.document()
    }

    /// The Document has changes since the last save or open.
    #[must_use]
    pub fn is_unsaved(&self) -> bool {
        self.session.is_unsaved()
    }

    /// A copy of the Document to save, and the undo step it is at. The next edit starts a
    /// new undo step, so it makes the Document unsaved again.
    pub(crate) fn snapshot(&mut self) -> (Document, Option<u64>) {
        self.session.snapshot()
    }

    pub(crate) fn mark_saved(&mut self, at: Option<u64>, cx: &mut Context<Self>) {
        self.session.mark_saved(at);
        cx.notify();
    }

    /// Shows `document` in place of the current one, with no edit history.
    pub(crate) fn load(&mut self, document: Document, cx: &mut Context<Self>) {
        *self = Self {
            focus_handle: self.focus_handle.clone(),
            ..Self::new(document, cx)
        };
        cx.notify();
    }

    /// Byte range of the selection; empty at the caret.
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.session.selection()
    }

    /// The Lab mark at the caret (see [`Document::lab_mark_at`]), which shows as selected.
    /// `None` when text is selected.
    #[must_use]
    pub fn selected_lab_mark(&self) -> Option<&LabMark> {
        let selection = self.selection();
        if !selection.is_empty() {
            return None;
        }
        self.document().lab_mark_at(selection.start)
    }

    /// The box around the rows of the selected Lab mark as last painted (window
    /// coordinates); `None` with no selected mark.
    #[must_use]
    pub fn selected_lab_mark_bounds(&self) -> Option<Bounds<Pixels>> {
        let mark = self.selected_lab_mark()?;
        self.painted
            .as_ref()?
            .text_quads(&mark.range, gpui::Hsla::default())
            .into_iter()
            .map(|quad| self.zoomed(quad.bounds))
            .reduce(|all, row| all.union(&row))
    }

    /// The Markers as last painted, in document order, each parent before the Slots in it.
    #[must_use]
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }

    /// The Variant dots of the Marker of `slot` as last painted (window coordinates), in
    /// Variant order; empty when it has none.
    #[must_use]
    pub fn marker_dots(&self, slot: SlotId) -> Vec<Bounds<Pixels>> {
        self.marker_dots
            .iter()
            .filter(|(id, _)| *id == slot)
            .map(|(_, dot)| *dot)
            .collect()
    }

    /// The tint boxes behind the hovered Slot and the Slot the Alternatives panel shows, as
    /// last painted (window coordinates); empty when there is neither.
    #[must_use]
    pub fn hover_tint(&self) -> Vec<Bounds<Pixels>> {
        self.hover_tint.clone()
    }

    /// The effects that play now, oldest first.
    #[must_use]
    pub fn effects(&self) -> Vec<PlayingEffect> {
        self.effects.playing()
    }

    /// Deletes Variant `index` of Slot `id` as one undo step, with an
    /// explosion at the span. Returns the range of the kept text when the Slot ends.
    pub(crate) fn delete_variant(
        &mut self,
        id: SlotId,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Result<Option<Range<usize>>, SlotError> {
        let range = self
            .session
            .document()
            .slot(id)
            .ok_or(SlotError::NoSlot(id))?
            .range;
        let kept = self.change_document(|document| document.delete_variant(id, index), cx)?;
        let len = self.session.document().text().len();
        self.effects.play(
            Effect::Explosion,
            range.start.min(len)..range.end.min(len),
            cx,
        );
        sounds::play(Sound::Explosion, cx);
        cx.emit(EditorEvent::VariantDeleted(id));
        Ok(kept)
    }

    /// With the caret in a Slot span, deletes its Active variant.
    fn delete_active_variant(
        &mut self,
        _: &DeleteActiveVariant,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self
            .session
            .document()
            .slot_at(self.caret_offset())
            .and_then(|id| self.session.document().slot(id))
        else {
            return;
        };
        let (id, active) = (slot.id, slot.active);
        self.delete_variant(id, active, cx).ok();
    }

    /// The centre line of the underline of `slot` as last painted (window coordinates), row
    /// by row; empty when it has none.
    #[must_use]
    pub fn underline(&self, slot: SlotId) -> Vec<Point<Pixels>> {
        self.underlines
            .iter()
            .filter(|stroke| stroke.slot == slot)
            .flat_map(|stroke| stroke.centre.iter().copied())
            .collect()
    }

    /// The Slot under the pointer, innermost first.
    #[must_use]
    pub fn hovered_slot(&self) -> Option<SlotId> {
        self.hovered
    }

    /// The rows of the open context menu; `None` when it is closed.
    #[must_use]
    pub fn context_menu_items(&self) -> Option<Vec<ContextMenuItem>> {
        self.context_menu.as_ref().map(ContextMenu::items)
    }

    /// The text cut at Slot, Ghost, Trim cut and bright paragraph edges: what a Slot
    /// underline needs.
    fn underline_spans(&self) -> Vec<Span> {
        let shown = self.shown_document();
        let underlined: Vec<Range<usize>> = shown
            .slots()
            .into_iter()
            .filter(|slot| slot.level != Level::Paragraph)
            .map(|slot| slot.range)
            .collect();
        spans(
            shown.text().len(),
            0,
            &underlined,
            &shown.ghost_ranges(),
            &self.trim_fades(),
            &[],
            self.bright_paragraph().as_ref(),
        )
    }

    /// The paragraph that stays bright while the Alternatives panel is open, or the one
    /// under a dragged Overflow block; the rest of the text dims. `None` when nothing dims.
    #[must_use]
    pub fn bright_paragraph(&self) -> Option<Range<usize>> {
        let shown = self.shown_document();
        let range = match (self.drop_target, self.spotlight.as_ref()) {
            (Some(at), _) => self.to_shown(&(at..at)),
            (None, Some(Spotlight::Slot(id))) => shown.slot(*id)?.range,
            (None, Some(Spotlight::Text(range))) => self.to_shown(range),
            (None, None) => return None,
        };
        Some(paragraph_around(shown.text(), &range))
    }

    /// The Document as painted: with the draft of the Alternatives panel in place of the
    /// text it is for, while there is one. Else the Document.
    #[must_use]
    pub fn shown_document(&self) -> &Document {
        self.preview.as_ref().unwrap_or(self.session.document())
    }

    /// Shows `text` in place of `range` until the next call; `None` shows the Document.
    /// The Document does not change.
    pub(crate) fn set_preview(
        &mut self,
        preview: Option<(Range<usize>, &str)>,
        cx: &mut Context<Self>,
    ) {
        self.preview = preview.and_then(|(range, text)| {
            let mut shown = self.session.document().without_history();
            shown.replace(range, text).ok().map(|()| shown)
        });
        cx.notify();
    }

    /// Where `range` of the Document lies in the shown Document.
    fn to_shown(&self, range: &Range<usize>) -> Range<usize> {
        let (old, new) = (self.session.document().text(), self.shown_document().text());
        follow_change(range.start, old, new)..follow_change(range.end, old, new)
    }

    pub(crate) fn set_spotlight(&mut self, spotlight: Option<Spotlight>, cx: &mut Context<Self>) {
        if self.spotlight != spotlight {
            self.spotlight = spotlight;
            cx.notify();
        }
    }

    /// The ranges of the cuts still in the Trim proposal, which fade.
    fn trim_fades(&self) -> Vec<Range<usize>> {
        self.session
            .document()
            .trim_proposal()
            .map_or_else(Vec::new, TrimProposal::open_ranges)
    }

    /// Counter text: the Document, or the selection with "selected".
    #[must_use]
    pub fn counter_label(&self) -> String {
        self.session.counter_label()
    }

    fn caret_offset(&self) -> usize {
        self.session.caret().offset()
    }

    /// Up/Down over a Slot, or with the caret in a Slot span, cycle its Active variant
    /// in place, as one undo step. `false` when there is no such Slot.
    fn cycle_hovered(&mut self, cycle: Cycle, cx: &mut Context<Self>) -> bool {
        let at_caret = || {
            self.session
                .selection()
                .is_empty()
                .then(|| self.session.document().slot_at(self.caret_offset()))
                .flatten()
        };
        let Some(id) = self.hovered.or_else(at_caret) else {
            return false;
        };
        let Ok(switch) = self.change_document(|document| document.cycle_active(id, cycle), cx)
        else {
            self.hovered = None;
            return false;
        };
        sounds::play(switch.sound(), cx);
        true
    }

    /// Starts the fade-in of `slot`, whose Active variant changed, `delay` from now.
    fn fade_in(&mut self, slot: Option<SlotId>, delay: Duration, cx: &App) {
        if let Some(slot) = slot {
            self.fade_in = Some((slot, cx.background_executor().now() + delay));
        }
    }

    /// The Slot whose new Active variant fades in now, and its text opacity. `None` when
    /// no fade runs; with reduced motion a fade is done at once.
    #[must_use]
    pub fn fading_in(&self, cx: &App) -> Option<(SlotId, f32)> {
        let (slot, started) = self.fade_in?;
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(started);
        let progress = Effect::VariantFadeIn.progress(
            elapsed,
            settings::is_on(cx, |settings| settings.reduced_motion),
        );
        (progress < 1.).then_some((
            slot,
            FADE_IN_START_OPACITY + (1. - FADE_IN_START_OPACITY) * progress,
        ))
    }

    /// Runs `change` on the Document as one undo step (see [`EditSession::change`]); a new
    /// Active variant fades in.
    pub(crate) fn change_document<T, E>(
        &mut self,
        change: impl FnOnce(&mut Document) -> Result<T, E>,
        cx: &mut Context<Self>,
    ) -> Result<T, E> {
        let (done, switched) = self.session.change(change)?;
        self.fade_in(switched, Duration::ZERO, cx);
        cx.notify();
        Ok(done)
    }

    /// A click on a faded word keeps its cut. `false` when no open cut is there.
    fn keep_trim_cut_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(glyph) = self
            .painted
            .as_ref()
            .and_then(|painted| painted.glyph_at(self.unzoomed(position) - painted.bounds.origin))
        else {
            return false;
        };
        let index = self
            .session
            .document()
            .trim_proposal()
            .and_then(|proposal| proposal.open_cut_at(glyph));
        index.is_some_and(|index| {
            self.change_document(|document| document.keep_trim_cut(index), cx)
                .is_ok()
        })
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.undo_step(cx);
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.redo_step(cx);
    }

    /// Reverts the last editor undo step: text edits and Overflow edits share one history.
    /// Nothing happens with nothing to undo.
    pub(crate) fn undo_step(&mut self, cx: &mut Context<Self>) {
        let Some(changes) = self.session.undo() else {
            return;
        };
        // The restored text fades in after its burst.
        let after_burst = if changes.restored.is_empty() || !effects::plays(cx) {
            Duration::ZERO
        } else {
            Effect::UndoBurst.duration()
        };
        for id in changes.restored {
            let Some(range) = self.session.document().slot(id).map(|slot| slot.range) else {
                continue;
            };
            self.effects.play(Effect::UndoBurst, range, cx);
            cx.emit(EditorEvent::VariantRestored(id));
        }
        self.fade_in(changes.switched, after_burst, cx);
        self.autoscroll = true;
        cx.notify();
    }

    /// Puts back the last undone editor undo step; the selection goes where it ended.
    /// Nothing happens with nothing to redo.
    pub(crate) fn redo_step(&mut self, cx: &mut Context<Self>) {
        let Some(changes) = self.session.redo() else {
            return;
        };
        let len = self.session.document().text().len();
        for span in &changes.deleted {
            self.effects.play(
                Effect::Explosion,
                span.start.min(len)..span.end.min(len),
                cx,
            );
        }
        if !changes.deleted.is_empty() {
            sounds::play(Sound::Explosion, cx);
        }
        self.fade_in(changes.switched, Duration::ZERO, cx);
        self.autoscroll = true;
        cx.notify();
    }

    /// Makes `edit` in the Overflow text. `false` when refused.
    pub(crate) fn edit_overflow(&mut self, edit: &CaretEdit, cx: &mut Context<Self>) -> bool {
        if !self.session.edit_overflow(edit) {
            return false;
        }
        cx.notify();
        true
    }

    /// Ends the current Undo step, so the next edit starts a new one.
    pub(crate) fn close_history(&mut self) {
        self.session.close_undo_step();
    }

    /// Ctrl+Return puts the Overflow block at Overflow offset `caret` at the caret.
    pub(crate) fn use_overflow_block(&mut self, caret: usize, cx: &mut Context<Self>) -> bool {
        let at = self.caret_offset();
        self.insert_overflow_block(at, |document| document.use_block(caret, at), cx)
    }

    /// A block dragged into the page goes in at the drop point.
    pub(crate) fn drop_overflow_block(
        &mut self,
        caret: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.drop_target = None;
        let at = caret::offset_at(self, position, cx);
        self.insert_overflow_block(at, |document| document.drop_block(caret, at), cx)
    }

    /// Where a dragged Overflow block goes in if it drops now; `None` with no drag over
    /// the page.
    #[must_use]
    pub fn drop_caret(&self) -> Option<usize> {
        self.drop_target
    }

    /// A dragged Overflow block is at window `position` over the page, or `None` off it.
    pub(crate) fn drag_block_over(
        &mut self,
        position: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let target = position.map(|position| caret::offset_at(self, position, cx));
        if self.drop_target != target {
            self.drop_target = target;
            cx.notify();
        }
    }

    /// Inserts a copy of an Overflow block at `at` with `insert`, as one undo step; the
    /// caret goes after it.
    fn insert_overflow_block(
        &mut self,
        at: usize,
        insert: impl FnOnce(&mut Document) -> Result<(), hamish_core::OverflowError>,
        cx: &mut Context<Self>,
    ) -> bool {
        let inserted = self.session.change_then_select(insert, |old, new| {
            let after = (at + new.text().len()).saturating_sub(old.len());
            after..after
        });
        if inserted.is_err() {
            return false;
        }
        self.autoscroll = true;
        cx.notify();
        true
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.keep_trim_cut_at(event.position, cx);
        self.selecting = true;
        if event.click_count == 2 {
            caret::select_word(self, event.position, cx);
        } else {
            caret::press(self, event.position, event.modifiers.shift, cx);
        }
        if let Some(slot) = self.slot_at(event.position) {
            cx.emit(EditorEvent::SlotClicked(slot));
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            caret::press(self, event.position, true, cx);
        }
        let hovered = self.slot_at(event.position);
        if hovered != self.hovered {
            self.hovered = hovered;
            cx.notify();
        }
    }

    /// The innermost visible Slot whose text (or Paragraph margin) is under `position`.
    fn slot_at(&self, position: Point<Pixels>) -> Option<SlotId> {
        let painted = self.painted.as_ref()?;
        let local = self.unzoomed(position) - painted.bounds.origin;
        let slots = self.session.document().slots();
        slots
            .iter()
            .rev()
            .find(|slot| element::covers(painted, &slot.range, slot.level, local))
            .map(|slot| slot.id)
    }

    /// What the menu actions act on: the selection, else the word at the caret,
    /// else the caret.
    pub(crate) fn target(&self) -> Range<usize> {
        self.session.target()
    }

    /// A right-click opens the context menu. Outside the selection it moves the
    /// caret there first, so the menu acts on the word under the pointer.
    fn on_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        let offset = caret::offset_at(self, event.position, cx);
        let in_selection =
            self.session.selection().start <= offset && offset <= self.session.selection().end;
        if self.session.selection().is_empty() || !in_selection {
            caret::press(self, event.position, false, cx);
        }
        let target = self.target();
        let in_ghost = self.session.document().touches_ghost(&target);
        if target.is_empty() && !in_ghost {
            self.context_menu = None;
            return;
        }
        let word = self
            .session
            .selection()
            .is_empty()
            .then(|| self.session.document().text().get(target))
            .flatten()
            .filter(|word| !word.is_empty())
            .map(str::to_owned);
        self.context_menu = Some(ContextMenu {
            position: event.position,
            word,
            in_ghost,
        });
        cx.notify();
    }

    /// Esc closes the context menu. With none open, Insert mode goes back to Normal mode;
    /// else a view up the tree takes it.
    fn close_menu(&mut self, _: &CloseMenu, _: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
            return;
        }
        let in_insert = self.vim_mode(cx) == Some(VimMode::Insert);
        if self.vim_mode(cx).is_some() {
            self.run_vim(VimKey::Escape, cx);
        }
        if !in_insert {
            cx.propagate();
        }
    }

    /// The Vim mode of the Editor; `None` when the Vim mode setting is off.
    #[must_use]
    pub fn vim_mode(&self, cx: &App) -> Option<VimMode> {
        vim_on(cx).then(|| self.vim.mode())
    }

    /// The Vim mode and the keys of a command not yet complete, as the Chrome bar shows
    /// them; `None` when the Vim mode setting is off.
    #[must_use]
    pub fn vim_status(&self, cx: &App) -> Option<String> {
        vim_on(cx).then(|| self.vim.status())
    }

    /// A key that no key binding took goes to Vim mode. A key Vim mode uses goes no further,
    /// so in Normal mode no key reaches the text input.
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if vim_on(cx) && self.run_vim(vim_key(&event.keystroke), cx) {
            cx.stop_propagation();
        }
    }

    /// Gives `key` to Vim mode and runs its command. `false` when Vim mode passes the key on.
    fn run_vim(&mut self, key: VimKey, cx: &mut Context<Self>) -> bool {
        let command = self
            .vim
            .key(key, self.session.document().text(), self.session.caret());
        match command {
            VimCommand::PassOn => return false,
            VimCommand::Nothing => {}
            VimCommand::Select(caret) => CaretView::select(self, caret, cx),
            VimCommand::Edit(edit) => CaretView::edit(self, &edit, cx),
        }
        cx.notify();
        true
    }

    /// Ghost the target, or Revive it when it touches a Ghost range, as one
    /// undo step.
    fn toggle_ghost(&mut self, _: &ToggleGhost, _: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        let target = self.target();
        self.session
            .change(|document| document.toggle_ghost(target))
            .ok();
        cx.notify();
    }

    /// Menu actions that later tickets build (Alternatives panel, AI alternatives):
    /// close the menu and let a view up the tree handle the action.
    fn pass_on(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        cx.notify();
        cx.propagate();
    }

    fn open_alternatives(&mut self, _: &OpenAlternatives, _: &mut Window, cx: &mut Context<Self>) {
        self.pass_on(cx);
    }

    fn ai_alternatives(&mut self, _: &AiAlternatives, _: &mut Window, cx: &mut Context<Self>) {
        self.pass_on(cx);
    }

    /// Moves the target to the end of Overflow as one undo step, then lets the
    /// workspace show Overflow.
    fn stash(&mut self, _: &Stash, _: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        cx.notify();
        let target = self.target();
        let stashed = self.session.change_then_select(
            |document| document.stash(target.clone()),
            |_, _| target.start..target.start,
        );
        if stashed.is_err() {
            return;
        }
        self.autoscroll = true;
        cx.propagate();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }
}

impl CaretView for Editor {
    fn read_text<R>(&self, _: &App, read: impl FnOnce(&str, &Caret) -> R) -> R {
        read(self.session.document().text(), self.session.caret())
    }

    fn select(&mut self, caret: Caret, cx: &mut Context<Self>) {
        self.session.select(caret);
        self.autoscroll = true;
        cx.notify();
    }

    fn unmark(&mut self, _: &mut Context<Self>) {
        self.session.unmark();
    }

    fn edit(&mut self, edit: &CaretEdit, cx: &mut Context<Self>) {
        if !self.session.edit(edit) {
            return;
        }
        self.context_menu = None;
        self.autoscroll = true;
        cx.notify();
    }

    fn painted(&self) -> Option<&PaintedText> {
        self.painted.as_ref()
    }

    fn column(&mut self) -> &mut Option<Column> {
        &mut self.column
    }

    fn to_painted(&self, position: Point<Pixels>) -> Point<Pixels> {
        self.unzoomed(position)
    }

    fn to_window(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        self.zoomed(bounds)
    }

    fn take_vertical_key(&mut self, cycle: Cycle, cx: &mut Context<Self>) -> bool {
        self.cycle_hovered(cycle, cx)
    }

    fn takes_text(&self, cx: &App) -> bool {
        self.vim_mode(cx) != Some(VimMode::Normal)
    }
}

fn vim_on(cx: &App) -> bool {
    settings::is_on(cx, |settings| settings.vim_mode)
}

/// `keystroke` as Vim mode reads it: the char it types, or a named key.
fn vim_key(keystroke: &gpui::Keystroke) -> VimKey {
    let modifiers = keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
        return VimKey::Other;
    }
    match keystroke.key.as_str() {
        "escape" => VimKey::Escape,
        "enter" => VimKey::Enter,
        "backspace" => VimKey::Backspace,
        _ => one_char(keystroke.key_char.as_deref()).map_or(VimKey::Other, VimKey::Char),
    }
}

/// The char of `typed` when it is exactly one char.
fn one_char(typed: Option<&str>) -> Option<char> {
    let mut chars = typed?.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

caret_input_handler!(Editor);

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A drag that ends off the page sends no event; the end of the drag clears it.
        if !cx.has_active_drag() {
            self.drop_target = None;
        }
        let theme = cx.global::<Theme>().clone();
        if !vim_on(cx) {
            self.vim = Vim::default();
        }
        let key_context = match self.vim_mode(cx) {
            None => "Editor",
            Some(VimMode::Normal) => "Editor VimNormal",
            Some(VimMode::Insert) => "Editor VimInsert",
        };
        let editor = div()
            .id("editor")
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam);
        caret_actions(editor, cx)
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::toggle_ghost))
            .on_action(cx.listener(Self::open_alternatives))
            .on_action(cx.listener(Self::ai_alternatives))
            .on_action(cx.listener(Self::stash))
            .on_action(cx.listener(Self::close_menu))
            .on_action(cx.listener(Self::delete_active_variant))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .font_family(theme.text_font.clone())
            .text_size(theme.font_size_body)
            .line_height(theme.line_height_body)
            .text_color(theme.text)
            .child(
                div()
                    .mx_auto()
                    .max_w(theme.column_width)
                    .w_full()
                    .pt(theme.column_top)
                    .pb(theme.column_top)
                    .child(EditorElement {
                        editor: cx.entity(),
                    }),
            )
            .when_some(self.context_menu.as_ref(), |editor, menu| {
                let dismiss = cx.listener(|editor: &mut Self, _: &MouseDownEvent, _, cx| {
                    editor.context_menu = None;
                    cx.notify();
                });
                editor.child(menu.render(&theme, dismiss))
            })
    }
}
