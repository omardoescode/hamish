use std::f32::consts::TAU;
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, Font, FontStyle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Path, PathBuilder, Pixels, Point, Render,
    ScrollHandle, SharedString, Size, Style, TextRun, UTF16Selection, Window, WrappedLine, div,
    fill, point, prelude::*, px, relative, size,
};
use ka_teb_core::{
    Cycle, Document, EditHistory, EditKind, Effect, FocusZoom, LabError, Level, MarkdownSpan,
    MarkdownStyle, Slot, SlotError, SlotId, Sound, TrimError, paragraph_around,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::Theme;
use crate::actions::{
    AiAlternatives, Backspace, CloseMenu, Copy, Cut, Delete, DeleteActiveVariant, MoveDown,
    MoveLeft, MoveRight, MoveToLineEnd, MoveToLineStart, MoveUp, Newline, OpenAlternatives, Paste,
    Redo, SelectAll, SelectDown, SelectLeft, SelectRight, SelectToLineEnd, SelectToLineStart,
    SelectUp, Stash, ToggleGhost, Undo,
};
use crate::context_menu::{self, ContextMenu, ContextMenuItem};
use crate::effects::{self, Particles, PlayingEffect};
use crate::settings_modal;
use crate::sounds;

mod focus_zoom;

/// How a Slot shows that it has Variants (FR-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// Word and Sentence: underline plus a row of Variant dots.
    Underline,
    /// Paragraph: a line in the left margin plus a column of Variant dots.
    MarginLine,
}

/// The Marker of one visible Slot: `dots` Variant dots, dot `active` filled (FR-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub slot: SlotId,
    pub kind: MarkerKind,
    pub dots: usize,
    pub active: usize,
}

const DOT_SIZE: Pixels = px(4.);
const DOT_GAP: Pixels = px(3.);
/// Paragraph Marker: a line about 2 px wide, about 14 px left of the text (spec).
const MARGIN_LINE_WIDTH: Pixels = px(2.);
const MARGIN_LINE_GAP: Pixels = px(14.);
/// How far left each nesting level moves a Paragraph margin line.
const MARGIN_LINE_STEP: Pixels = px(10.);
const HOVER_PAD: Pixels = px(2.);
const HOVER_RADIUS: Pixels = px(3.);

/// Word and Sentence underline: an editor's pen, a little wavy (issue #28). Height of the
/// wobble, length of its main wave, base thickness and how much the thickness changes.
const PEN_WOBBLE: f32 = 0.8;
const PEN_WAVELENGTH: f32 = 60.;
const PEN_THICKNESS: f32 = 1.;
const PEN_THICKNESS_WOBBLE: f32 = 0.25;
/// Distance between the points of the pen line.
const PEN_STEP: Pixels = px(2.);

/// Width left of the text that a Paragraph Marker and its hover area take.
const PARAGRAPH_MARGIN: Pixels = px(32.);

/// Opacity of new Active variant text when its fade-in starts (FR-81: "low opacity").
const FADE_IN_START_OPACITY: f32 = 0.2;

/// The text column: our own GPUI editor element over a core [`Document`] (ADR 0002).
pub struct Editor {
    document: Document,
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    history: EditHistory,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    painted: Option<PaintedText>,
    selecting: bool,
    autoscroll: bool,
    hovered: Option<SlotId>,
    markers: Vec<Marker>,
    underlines: Vec<PenStroke>,
    context_menu: Option<ContextMenu>,
    spotlight: Option<Spotlight>,
    /// The Document with the draft of the Alternatives panel in place (FR-68); it is painted,
    /// never saved.
    preview: Option<Document>,
    /// The undo step the last save wrote (see [`EditHistory::top`]).
    saved: Option<u64>,
    /// The Slot whose Active variant last changed, and when (FR-81).
    fade_in: Option<(SlotId, Instant)>,
    /// FR-78, FR-79: effects that play over the text.
    effects: Vec<Particles>,
    effect_seed: u64,
    focus_zoom: FocusZoom,
    /// The zoomed paragraphs as last painted (FR-83).
    zooms: Vec<focus_zoom::Zoom>,
}

/// What the open Alternatives panel shows: its paragraph stays bright, the rest dims (FR-27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Spotlight {
    Slot(SlotId),
    Text(Range<usize>),
}

/// What shows of a Slot: its Active variant (index and text) and its Variant count.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ShownVariant {
    slot: SlotId,
    active: usize,
    count: usize,
    text: String,
}

impl ShownVariant {
    /// A different Variant is active than in `old`. Typing in the Active variant changes
    /// only its text; an added Variant changes only the count. Neither is a new Active variant.
    fn replaces(&self, old: &Self) -> bool {
        let moved = self.active != old.active || self.count != old.count;
        moved && self.text != old.text
    }
}

/// What the editor tells the views around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorEvent {
    /// A click on the text of a visible Slot (FR-21).
    SlotClicked(SlotId),
    /// A Variant of the Slot was deleted; its explosion and its sound (#27) play (FR-78).
    VariantDeleted(SlotId),
    /// An undo put a deleted Variant of the Slot back; its burst plays (FR-79).
    VariantRestored(SlotId),
}

impl EventEmitter<EditorEvent> for Editor {}

/// The text as last painted, for mouse and vertical cursor moves.
pub(crate) struct PaintedText {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) line_height: Pixels,
    pub(crate) lines: Vec<PaintedLine>,
}

pub(crate) struct PaintedLine {
    start: usize,
    top: Pixels,
    line: WrappedLine,
}

impl Editor {
    pub fn new(document: Document, cx: &mut Context<Self>) -> Self {
        Self {
            document,
            selection: 0..0,
            reversed: false,
            marked: None,
            history: EditHistory::default(),
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            painted: None,
            selecting: false,
            autoscroll: false,
            hovered: None,
            markers: Vec::new(),
            underlines: Vec::new(),
            context_menu: None,
            spotlight: None,
            preview: None,
            saved: None,
            fade_in: None,
            effects: Vec::new(),
            effect_seed: 0,
            focus_zoom: FocusZoom::default(),
            zooms: Vec::new(),
        }
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The Document has changes since the last save or open.
    #[must_use]
    pub fn is_unsaved(&self) -> bool {
        self.history.top() != self.saved
    }

    /// A copy of the Document to save, and the undo step it is at. The next edit starts a
    /// new undo step, so it makes the Document unsaved again.
    pub(crate) fn snapshot(&mut self) -> (Document, Option<u64>) {
        self.history.close();
        (self.document.clone(), self.history.top())
    }

    pub(crate) fn mark_saved(&mut self, at: Option<u64>, cx: &mut Context<Self>) {
        self.saved = at;
        cx.notify();
    }

    /// Shows `document` in place of the current one, with no edit history.
    pub(crate) fn load(&mut self, document: Document, cx: &mut Context<Self>) {
        *self = Self {
            document,
            focus_handle: self.focus_handle.clone(),
            ..Self::new(Document::default(), cx)
        };
        cx.notify();
    }

    /// Byte range of the selection; empty at the cursor.
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// The Markers as last painted, in document order, each parent before the Slots in it.
    #[must_use]
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }

    /// The effects that play now, oldest first (FR-78, FR-79).
    #[must_use]
    pub fn effects(&self) -> Vec<PlayingEffect> {
        self.effects
            .iter()
            .map(|particles| particles.playing.clone())
            .collect()
    }

    fn play(&mut self, effect: Effect, range: Range<usize>, cx: &App) {
        if !effects::plays(cx) {
            return;
        }
        self.effect_seed = self.effect_seed.wrapping_add(1);
        self.effects.push(Particles::new(
            PlayingEffect { effect, range },
            self.effect_seed,
        ));
    }

    /// FR-77, FR-80: deletes Variant `index` of Slot `id` as one undo step, with an
    /// explosion at the span (FR-78).
    pub(crate) fn delete_variant(
        &mut self,
        id: SlotId,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Result<(), SlotError> {
        let range = self.document.slot(id).ok_or(SlotError::NoSlot(id))?.range;
        self.change_document(|document| document.delete_variant(id, index), cx)?;
        let len = self.document.text().len();
        self.play(
            Effect::Explosion,
            range.start.min(len)..range.end.min(len),
            cx,
        );
        sounds::play(Sound::Explosion, cx);
        cx.emit(EditorEvent::VariantDeleted(id));
        Ok(())
    }

    /// FR-77: with the caret in a Slot span, deletes its Active variant.
    fn delete_active_variant(
        &mut self,
        _: &DeleteActiveVariant,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self
            .document
            .slot_at(self.cursor())
            .and_then(|id| self.document.slot(id))
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

    /// The paragraph that stays bright while the Alternatives panel is open (FR-27); the
    /// rest of the text dims. `None` when nothing dims.
    #[must_use]
    pub fn bright_paragraph(&self) -> Option<Range<usize>> {
        let shown = self.shown_document();
        let range = match self.spotlight.as_ref()? {
            Spotlight::Slot(id) => shown.slot(*id)?.range,
            Spotlight::Text(range) => self.to_shown(range),
        };
        Some(paragraph_around(shown.text(), &range))
    }

    /// The Document as painted: with the draft of the Alternatives panel in place of the
    /// text it is for, while there is one (FR-68). Else the Document.
    #[must_use]
    pub fn shown_document(&self) -> &Document {
        self.preview.as_ref().unwrap_or(&self.document)
    }

    /// Shows `text` in place of `range` until the next call; `None` shows the Document.
    /// The Document does not change.
    pub(crate) fn set_preview(
        &mut self,
        preview: Option<(Range<usize>, &str)>,
        cx: &mut Context<Self>,
    ) {
        self.preview = preview.and_then(|(range, text)| {
            let mut shown = self.document.clone();
            shown.replace(range, text).ok().map(|()| shown)
        });
        cx.notify();
    }

    /// Where `range` of the Document lies in the shown Document.
    fn to_shown(&self, range: &Range<usize>) -> Range<usize> {
        let (old, new) = (self.document.text(), self.shown_document().text());
        follow_change(range.start, old, new)..follow_change(range.end, old, new)
    }

    pub(crate) fn set_spotlight(&mut self, spotlight: Option<Spotlight>, cx: &mut Context<Self>) {
        if self.spotlight != spotlight {
            self.spotlight = spotlight;
            cx.notify();
        }
    }

    /// The ranges of the cuts still in the Trim proposal, which fade (FR-55).
    fn trim_fades(&self) -> Vec<Range<usize>> {
        self.document
            .trim_proposal()
            .map_or_else(Vec::new, |proposal| {
                proposal
                    .cuts()
                    .iter()
                    .filter(|cut| !cut.kept)
                    .map(|cut| cut.range.clone())
                    .collect()
            })
    }

    /// Counter text: the Document, or the selection with "selected" (FR-3, FR-4).
    #[must_use]
    pub fn counter_label(&self) -> String {
        if self.selection.is_empty() {
            return self.document.counter().to_string();
        }
        match self.document.selection_counter(self.selection.clone()) {
            Ok(counter) => format!("{counter} selected"),
            Err(_) => self.document.counter().to_string(),
        }
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selection = offset..offset;
        self.reversed = false;
        self.history.close();
        self.autoscroll = true;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.selection.start = offset;
        } else {
            self.selection.end = offset;
        }
        if self.selection.end < self.selection.start {
            self.reversed = !self.reversed;
            self.selection = self.selection.end..self.selection.start;
        }
        self.history.close();
        self.autoscroll = true;
        cx.notify();
    }

    fn edit(&mut self, range: Range<usize>, text: &str, kind: EditKind, cx: &mut Context<Self>) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        if self.document.replace(range.clone(), text).is_err() {
            return;
        }
        self.context_menu = None;
        let before = self.selection.clone();
        let cursor = range.start + text.len();
        self.selection = cursor..cursor;
        self.reversed = false;
        self.history.record(kind, before, self.selection.clone());
        self.autoscroll = true;
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.document
            .text()
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let text = self.document.text();
        text.grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(text.len())
    }

    /// The offset `rows` visual rows above (negative) or below the cursor.
    fn offset_rows_away(&self, rows: f32) -> usize {
        let cursor = self.cursor();
        let Some(painted) = self.painted.as_ref() else {
            return cursor;
        };
        let Some(position) = painted.position_for(cursor) else {
            return cursor;
        };
        let y = position.y + painted.line_height * (rows + 0.5);
        painted.offset_for_local(point(position.x, y), self.document.text().len())
    }

    /// The offset at the start (`x` = 0) or end (`x` = MAX) of the cursor's visual row.
    fn offset_in_row(&self, x: Pixels) -> usize {
        let cursor = self.cursor();
        let Some(painted) = self.painted.as_ref() else {
            return cursor;
        };
        let Some(position) = painted.position_for(cursor) else {
            return cursor;
        };
        let y = position.y + painted.line_height * 0.5;
        painted.offset_for_local(point(x, y), self.document.text().len())
    }

    fn offset_for_mouse(&self, position: Point<Pixels>) -> usize {
        let Some(painted) = self.painted.as_ref() else {
            return self.cursor();
        };
        let local = self.unzoomed(position) - painted.bounds.origin;
        painted.offset_for_local(local, self.document.text().len())
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selection.is_empty() {
            self.previous_boundary(self.cursor())
        } else {
            self.selection.start
        };
        self.move_to(offset, cx);
    }

    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selection.is_empty() {
            self.next_boundary(self.cursor())
        } else {
            self.selection.end
        };
        self.move_to(offset, cx);
    }

    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if !self.cycle_hovered(Cycle::Previous, cx) {
            self.move_to(self.offset_rows_away(-1.), cx);
        }
    }

    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if !self.cycle_hovered(Cycle::Next, cx) {
            self.move_to(self.offset_rows_away(1.), cx);
        }
    }

    /// FR-14: Up/Down over a Slot, or with the caret in a Slot span, cycle its Active variant
    /// in place, as one undo step. `false` when there is no such Slot.
    fn cycle_hovered(&mut self, cycle: Cycle, cx: &mut Context<Self>) -> bool {
        let at_caret = || {
            self.selection
                .is_empty()
                .then(|| self.document.slot_at(self.cursor()))
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

    /// Each Slot with what shows of it: see [`ShownVariant`].
    fn shown_variants(&self) -> Vec<ShownVariant> {
        self.document
            .slots()
            .iter()
            .map(|slot| ShownVariant {
                slot: slot.id,
                active: slot.active,
                count: slot.variants.len(),
                text: slot
                    .variants
                    .get(slot.active)
                    .map_or_default(|variant| variant.text().to_owned()),
            })
            .collect()
    }

    /// Starts the fade-in of a Slot whose Active variant changed since `before` (cycle,
    /// delete, undo, redo), `delay` from now. See [`ShownVariant::replaces`].
    fn fade_in_changed(&mut self, before: &[ShownVariant], delay: Duration, cx: &App) {
        let changed = self.shown_variants().into_iter().find(|shown| {
            before
                .iter()
                .any(|old| old.slot == shown.slot && shown.replaces(old))
        });
        if let Some(shown) = changed {
            self.fade_in = Some((shown.slot, cx.background_executor().now() + delay));
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
            settings_modal::is_on(cx, |settings| settings.reduced_motion),
        );
        (progress < 1.).then_some((
            slot,
            FADE_IN_START_OPACITY + (1. - FADE_IN_START_OPACITY) * progress,
        ))
    }

    /// Runs one Document API call that makes at most one core undo step, as one editor
    /// undo step. The selection follows the text change.
    pub(crate) fn change_document<T, E>(
        &mut self,
        change: impl FnOnce(&mut Document) -> Result<T, E>,
        cx: &mut Context<Self>,
    ) -> Result<T, E> {
        self.change_document_in_steps(1, change, cx)
    }

    /// Like [`Self::change_document`], for a change of `steps` core undo steps.
    pub(crate) fn change_document_in_steps<T, E>(
        &mut self,
        steps: usize,
        change: impl FnOnce(&mut Document) -> Result<T, E>,
        cx: &mut Context<Self>,
    ) -> Result<T, E> {
        let old_text = self.document.text().to_owned();
        let shown = self.shown_variants();
        let done = change(&mut self.document)?;
        self.fade_in_changed(&shown, Duration::ZERO, cx);
        let new_text = self.document.text();
        let before = self.selection.clone();
        self.selection = follow_change(before.start, &old_text, new_text)
            ..follow_change(before.end, &old_text, new_text);
        self.history
            .record_steps(steps, before, self.selection.clone());
        cx.notify();
        Ok(done)
    }

    /// Runs a Lab `step` on the Document as one undo step; the selection follows a text
    /// change. A step that changes nothing records no undo step.
    pub(crate) fn lab_step<T>(
        &mut self,
        step: impl FnOnce(&mut Document) -> Result<T, LabError>,
        cx: &mut Context<Self>,
    ) -> Result<T, LabError> {
        let old_text = self.document.text().to_owned();
        let old_marks = self.document.lab_marks().to_vec();
        let result = step(&mut self.document)?;
        let new_text = self.document.text();
        if new_text == old_text && self.document.lab_marks() == old_marks.as_slice() {
            return Ok(result);
        }
        let before = self.selection.clone();
        self.selection = follow_change(before.start, &old_text, new_text)
            ..follow_change(before.end, &old_text, new_text);
        self.reversed = false;
        self.history.close();
        self.history
            .record(EditKind::Other, before, self.selection.clone());
        self.history.close();
        cx.notify();
        Ok(result)
    }

    /// Runs a Trim `step` on the Document. A step that changes text is one undo step and
    /// the selection follows it; a step that only changes the proposal records none.
    pub(crate) fn trim_step<T>(
        &mut self,
        step: impl FnOnce(&mut Document) -> Result<T, TrimError>,
        cx: &mut Context<Self>,
    ) -> Result<T, TrimError> {
        let old_text = self.document.text().to_owned();
        let result = step(&mut self.document)?;
        let new_text = self.document.text();
        if new_text != old_text {
            let before = self.selection.clone();
            self.selection = follow_change(before.start, &old_text, new_text)
                ..follow_change(before.end, &old_text, new_text);
            self.reversed = false;
            self.history.close();
            self.history
                .record(EditKind::Other, before, self.selection.clone());
            self.history.close();
        }
        cx.notify();
        Ok(result)
    }

    /// FR-57: a click on a faded word keeps its cut. `false` when no open cut is there.
    fn keep_trim_cut_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(glyph) = self
            .painted
            .as_ref()
            .and_then(|painted| painted.glyph_at(self.unzoomed(position) - painted.bounds.origin))
        else {
            return false;
        };
        let index = self.document.trim_proposal().and_then(|proposal| {
            proposal
                .cuts()
                .iter()
                .position(|cut| !cut.kept && cut.range.contains(&glyph))
        });
        index.is_some_and(|index| {
            self.trim_step(|document| document.keep_trim_cut(index), cx)
                .is_ok()
        })
    }

    fn move_to_line_start(&mut self, _: &MoveToLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.offset_in_row(px(0.)), cx);
    }

    fn move_to_line_end(&mut self, _: &MoveToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.offset_in_row(Pixels::MAX), cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.offset_rows_away(-1.), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.offset_rows_away(1.), cx);
    }

    fn select_to_line_start(
        &mut self,
        _: &SelectToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.offset_in_row(px(0.)), cx);
    }

    fn select_to_line_end(&mut self, _: &SelectToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.offset_in_row(Pixels::MAX), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.document.text().len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection.is_empty() {
            self.edit(self.selection.clone(), "", EditKind::Other, cx);
            return;
        }
        let cursor = self.cursor();
        self.edit(
            self.previous_boundary(cursor)..cursor,
            "",
            EditKind::Deleting,
            cx,
        );
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection.is_empty() {
            self.edit(self.selection.clone(), "", EditKind::Other, cx);
            return;
        }
        let cursor = self.cursor();
        self.edit(
            cursor..self.next_boundary(cursor),
            "",
            EditKind::Deleting,
            cx,
        );
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(self.selection.clone(), "\n", EditKind::Other, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.document.text().get(self.selection.clone()) else {
            return;
        };
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        self.edit(self.selection.clone(), "", EditKind::Other, cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.edit(self.selection.clone(), &text, EditKind::Other, cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.undo_step(cx);
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.redo_step(cx);
    }

    /// Reverts the last editor undo step: text edits and Overflow edits share one history.
    pub(crate) fn undo_step(&mut self, cx: &mut Context<Self>) {
        let Some(group) = self.history.undo() else {
            return;
        };
        let shown = self.shown_variants();
        let mut restored = Vec::new();
        for _ in 0..group.steps {
            restored.extend(self.document.undo_restores_variant());
            self.document.undo();
        }
        // FR-79: the restored text fades in after its burst.
        let after_burst = if restored.is_empty() || !effects::plays(cx) {
            Duration::ZERO
        } else {
            Effect::UndoBurst.duration()
        };
        for id in restored {
            let Some(range) = self.document.slot(id).map(|slot| slot.range) else {
                continue;
            };
            self.play(Effect::UndoBurst, range, cx);
            cx.emit(EditorEvent::VariantRestored(id));
        }
        self.fade_in_changed(&shown, after_burst, cx);
        self.selection = group.before;
        self.reversed = false;
        self.autoscroll = true;
        cx.notify();
    }

    pub(crate) fn redo_step(&mut self, cx: &mut Context<Self>) {
        let Some(group) = self.history.redo() else {
            return;
        };
        let shown = self.shown_variants();
        for _ in 0..group.steps {
            self.document.redo();
        }
        self.fade_in_changed(&shown, Duration::ZERO, cx);
        self.selection = group.after;
        self.reversed = false;
        self.autoscroll = true;
        cx.notify();
    }

    /// FR-46: replaces `range` of the Overflow text with `text`. `false` when refused.
    pub(crate) fn edit_overflow(
        &mut self,
        range: Range<usize>,
        text: &str,
        kind: EditKind,
        cx: &mut Context<Self>,
    ) -> bool {
        if range.is_empty() && text.is_empty() {
            return false;
        }
        if self.document.edit_overflow(range, text).is_err() {
            return false;
        }
        self.history
            .record(kind, self.selection.clone(), self.selection.clone());
        cx.notify();
        true
    }

    /// Ends the current undo group, so the next edit starts a new step.
    pub(crate) fn close_history(&mut self) {
        self.history.close();
    }

    /// FR-47: Ctrl+Return puts the Overflow block at Overflow offset `caret` at the cursor.
    pub(crate) fn use_overflow_block(&mut self, caret: usize, cx: &mut Context<Self>) -> bool {
        self.insert_overflow_block(caret, self.cursor(), cx)
    }

    /// FR-47: a block dragged into the page goes in at the drop point.
    pub(crate) fn drop_overflow_block(
        &mut self,
        caret: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.insert_overflow_block(caret, self.offset_for_mouse(position), cx)
    }

    /// Inserts a copy of the block at Overflow offset `caret` at `at`, as one undo step;
    /// the cursor goes after it.
    fn insert_overflow_block(&mut self, caret: usize, at: usize, cx: &mut Context<Self>) -> bool {
        let len = self.document.text().len();
        if self.document.use_block(caret, at).is_err() {
            return false;
        }
        let before = self.selection.clone();
        let cursor = at + self.document.text().len() - len;
        self.selection = cursor..cursor;
        self.reversed = false;
        self.history.close();
        self.history
            .record(EditKind::Other, before, self.selection.clone());
        self.history.close();
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
        let offset = self.offset_for_mouse(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
        if let Some(slot) = self.slot_at(event.position) {
            cx.emit(EditorEvent::SlotClicked(slot));
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.offset_for_mouse(event.position), cx);
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
        let slots = self.document.slots();
        slots
            .iter()
            .rev()
            .find(|slot| painted.covers(&slot.range, slot.level, local))
            .map(|slot| slot.id)
    }

    /// What the menu actions act on: the selection, else the word at the cursor (FR-29),
    /// else the cursor.
    pub(crate) fn target(&self) -> Range<usize> {
        if !self.selection.is_empty() {
            return self.selection.clone();
        }
        let cursor = self.cursor();
        context_menu::word_at(self.document.text(), cursor).unwrap_or(cursor..cursor)
    }

    /// FR-28: a right-click opens the context menu. Outside the selection it moves the
    /// cursor there first, so the menu acts on the word under the pointer.
    fn on_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        let offset = self.offset_for_mouse(event.position);
        let in_selection = self.selection.start <= offset && offset <= self.selection.end;
        if self.selection.is_empty() || !in_selection {
            self.move_to(offset, cx);
        }
        let target = self.target();
        let in_ghost = context_menu::touches_ghost(&self.document.ghost_ranges(), &target);
        if target.is_empty() && !in_ghost {
            self.context_menu = None;
            return;
        }
        let word = self
            .selection
            .is_empty()
            .then(|| self.document.text().get(target))
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

    fn close_menu(&mut self, _: &CloseMenu, _: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    /// FR-37, FR-39: Ghost the target, or Revive it when it touches a Ghost range, as one
    /// undo step.
    fn toggle_ghost(&mut self, _: &ToggleGhost, _: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        let target = self.target();
        let result = if context_menu::touches_ghost(&self.document.ghost_ranges(), &target) {
            self.document.revive(target)
        } else {
            self.document.ghost(target)
        };
        cx.notify();
        if result.is_err() {
            return;
        }
        self.history.close();
        self.history.record(
            EditKind::Other,
            self.selection.clone(),
            self.selection.clone(),
        );
        self.history.close();
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

    /// FR-45: moves the target to the end of Overflow as one undo step, then lets the
    /// workspace show Overflow.
    fn stash(&mut self, _: &Stash, _: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        cx.notify();
        let target = self.target();
        if self.document.stash(target.clone()).is_err() {
            return;
        }
        let before = self.selection.clone();
        self.selection = target.start..target.start;
        self.reversed = false;
        self.history.close();
        self.history
            .record(EditKind::Other, before, self.selection.clone());
        self.history.close();
        self.autoscroll = true;
        cx.propagate();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.document.text().chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.document.text().chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn input_range(&self, range_utf16: Option<&Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone())
    }
}

impl PaintedText {
    /// Height of all lines.
    pub(crate) fn height(&self) -> Pixels {
        self.lines.last().map_or(self.line_height, |last| {
            last.top + last.line.size(self.line_height).height
        })
    }

    /// Paints the lines from the top left of `bounds`. A failed glyph paint leaves that line
    /// blank; the next frame tries again.
    pub(crate) fn paint(&self, window: &mut Window, cx: &mut App) {
        for painted in &self.lines {
            let origin = self.bounds.origin + point(px(0.), painted.top);
            painted
                .line
                .paint(
                    origin,
                    self.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
        }
    }

    /// Position of `offset`, relative to the text origin.
    pub(crate) fn position_for(&self, offset: usize) -> Option<Point<Pixels>> {
        let painted = self.lines.iter().rev().find(|line| line.start <= offset)?;
        let position = painted
            .line
            .position_for_index(offset - painted.start, self.line_height)?;
        Some(point(position.x, painted.top + position.y))
    }

    /// Offset closest to `local` (relative to the text origin). Above the text is 0,
    /// below it is `len`.
    pub(crate) fn offset_for_local(&self, local: Point<Pixels>, len: usize) -> usize {
        if local.y < px(0.) {
            return 0;
        }
        let Some(painted) = self.lines.iter().rev().find(|line| line.top <= local.y) else {
            return 0;
        };
        let height = painted.line.size(self.line_height).height;
        if local.y >= painted.top + height {
            return len;
        }
        let row_point = point(local.x.max(px(0.)), local.y - painted.top);
        let index = match painted
            .line
            .closest_index_for_position(row_point, self.line_height)
        {
            Ok(index) | Err(index) => index,
        };
        painted.start + index
    }

    /// Offset of the glyph under `local` (relative to the text origin); `None` off the glyphs.
    fn glyph_at(&self, local: Point<Pixels>) -> Option<usize> {
        let painted = self
            .lines
            .iter()
            .rev()
            .find(|line| line.top <= local.y && local.x >= px(0.))?;
        let row_point = point(local.x, local.y - painted.top);
        let index = painted
            .line
            .index_for_position(row_point, self.line_height)
            .ok()?;
        Some(painted.start + index)
    }

    /// Rows from the row of `range.start` to the row of `range.end` (local tops).
    fn rows(&self, range: &Range<usize>) -> Option<(Point<Pixels>, Point<Pixels>)> {
        Some((
            self.position_for(range.start)?,
            self.position_for(range.end)?,
        ))
    }

    /// `local` lies on the text of `range`. A Paragraph also covers its full rows and
    /// its left margin, where its Marker is.
    fn covers(&self, range: &Range<usize>, level: Level, local: Point<Pixels>) -> bool {
        let Some((start, end)) = self.rows(range) else {
            return false;
        };
        if local.y < start.y || local.y >= end.y + self.line_height {
            return false;
        }
        if level == Level::Paragraph {
            return local.x >= -PARAGRAPH_MARGIN && local.x <= self.bounds.size.width;
        }
        let first_row = local.y < start.y + self.line_height;
        let last_row = local.y >= end.y;
        let left = if first_row { start.x } else { px(0.) };
        let right = if last_row {
            end.x
        } else {
            self.bounds.size.width
        };
        local.x >= left && local.x < right
    }

    /// The hover box behind `range` (window coordinates): one rounded box per row for a
    /// Word or Sentence Slot, one box over the full rows for a Paragraph (FR-14).
    fn hover_quads(&self, range: &Range<usize>, level: Level, color: Hsla) -> Vec<PaintQuad> {
        let Some((start, end)) = self.rows(range) else {
            return Vec::new();
        };
        let origin = self.bounds.origin;
        if level == Level::Paragraph {
            let bounds = Bounds::from_corners(
                origin + point(-HOVER_PAD * 2., start.y),
                origin
                    + point(
                        self.bounds.size.width + HOVER_PAD * 2.,
                        end.y + self.line_height,
                    ),
            );
            return vec![fill(bounds, color).corner_radii(HOVER_RADIUS)];
        }
        self.selection_quads(range, color)
            .into_iter()
            .map(|quad| {
                let bounds = Bounds::from_corners(
                    quad.bounds.origin - point(HOVER_PAD, px(0.)),
                    quad.bounds.bottom_right() + point(HOVER_PAD, px(0.)),
                );
                fill(bounds, color).corner_radii(HOVER_RADIUS)
            })
            .collect()
    }

    /// The Marker of a Slot over `range` (window coordinates), less the underline, which is
    /// a text run. `depth`: how many Paragraph Slots hold it; each moves a margin line left.
    fn marker_quads(
        &self,
        range: &Range<usize>,
        marker: &Marker,
        depth: usize,
        theme: &Theme,
    ) -> Vec<PaintQuad> {
        let Some((start, end)) = self.rows(range) else {
            return Vec::new();
        };
        let origin = self.bounds.origin;
        let dots_length = (DOT_SIZE + DOT_GAP) * as_factor(marker.dots) - DOT_GAP;
        let bottom = end.y + self.line_height;
        let row_left = if start.y == end.y { start.x } else { px(0.) };
        let line_x = -MARGIN_LINE_GAP - MARGIN_LINE_STEP * as_factor(depth);
        let (first_dot, dot_step, line) = match marker.kind {
            MarkerKind::Underline => (
                point(
                    (row_left + end.x - dots_length) / 2.,
                    bottom - DOT_SIZE - px(1.),
                ),
                point(DOT_SIZE + DOT_GAP, px(0.)),
                None,
            ),
            MarkerKind::MarginLine => (
                point(
                    line_x - DOT_GAP - DOT_SIZE,
                    (start.y + bottom - dots_length) / 2.,
                ),
                point(px(0.), DOT_SIZE + DOT_GAP),
                Some(fill(
                    Bounds::from_corners(
                        origin + point(line_x, start.y),
                        origin + point(line_x + MARGIN_LINE_WIDTH, bottom),
                    ),
                    theme.slot_paragraph_line,
                )),
            ),
        };
        let dots = (0..marker.dots).map(|index| {
            let at = origin + first_dot + dot_step * as_factor(index);
            let quad = fill(
                Bounds::new(at, size(DOT_SIZE, DOT_SIZE)),
                theme.slot_dot_filled,
            )
            .corner_radii(DOT_SIZE / 2.);
            if index == marker.active {
                return quad;
            }
            quad.background(gpui::transparent_black())
                .border_widths(px(1.))
                .border_color(theme.slot_dot)
        });
        line.into_iter().chain(dots).collect()
    }

    /// The pen strokes under `range` of `text` (window coordinates), one per row. The wobble
    /// depends only on the distance from the start of `slot`, so it stays the same on every
    /// paint and when text before the Slot changes.
    fn pen_strokes(
        &self,
        text: &str,
        range: &Range<usize>,
        slot: &Slot,
        color: Hsla,
    ) -> Vec<PenStroke> {
        let Some(slot_start) = self.position_for(slot.range.start) else {
            return Vec::new();
        };
        let seed = BuildHasherDefault::<DefaultHasher>::default().hash_one(slot.id);
        let width = f32::from(self.bounds.size.width);
        self.pen_rows(text, range)
            .into_iter()
            .filter(|row| row.left < row.right)
            .map(|row| {
                let along = f32::from(row.left - slot_start.x)
                    + (row.top - slot_start.y) / self.line_height * width;
                let mut stroke = PenStroke {
                    slot: slot.id,
                    centre: Vec::new(),
                    thickness: Vec::new(),
                    color,
                };
                let baseline = self.underline_y(row.top);
                let mut x = row.left;
                loop {
                    let (offset, thickness) = pen(seed, along + f32::from(x - row.left));
                    stroke
                        .centre
                        .push(self.bounds.origin + point(x, baseline + px(offset)));
                    stroke.thickness.push(px(thickness));
                    if x >= row.right {
                        return stroke;
                    }
                    x = (x + PEN_STEP).min(row.right);
                }
            })
            .collect()
    }

    /// The rows `range` of `text` covers, each from its first to its last char edge.
    fn pen_rows(&self, text: &str, range: &Range<usize>) -> Vec<PenRow> {
        let offsets = text
            .get(range.clone())
            .unwrap_or_default()
            .char_indices()
            .map(|(index, _)| range.start + index)
            .chain([range.end]);
        let mut rows: Vec<PenRow> = Vec::new();
        for at in offsets.filter_map(|offset| self.position_for(offset)) {
            if let Some(row) = rows.last_mut().filter(|row| row.top == at.y) {
                row.right = at.x;
                continue;
            }
            let left = if rows.is_empty() { at.x } else { px(0.) };
            rows.push(PenRow {
                top: at.y,
                left,
                right: at.x,
            });
        }
        rows
    }

    /// Local y of the underline on the row at `top`: where gpui puts a text underline.
    fn underline_y(&self, top: Pixels) -> Pixels {
        let Some(painted) = self.lines.iter().rev().find(|line| line.top <= top) else {
            return top + self.line_height;
        };
        let (ascent, descent) = (painted.line.ascent(), painted.line.descent());
        top + (self.line_height - ascent - descent) / 2. + ascent + descent * 0.618
    }

    /// Quads that cover `range` (window coordinates), one per visual row.
    /// The effects that still play at `now`, each with its progress and the rows of its span.
    fn effect_frames(
        &self,
        effects: &[Particles],
        now: Instant,
    ) -> Vec<(Particles, f32, Vec<Bounds<Pixels>>)> {
        effects
            .iter()
            .filter_map(|playing| {
                let progress = playing.progress(now)?;
                let rows = self
                    .selection_quads(&playing.playing.range, Hsla::default())
                    .into_iter()
                    .map(|quad| quad.bounds)
                    .collect();
                Some((playing.clone(), progress, rows))
            })
            .collect()
    }

    pub(crate) fn selection_quads(&self, range: &Range<usize>, color: Hsla) -> Vec<PaintQuad> {
        let (Some(start), Some(end)) =
            (self.position_for(range.start), self.position_for(range.end))
        else {
            return Vec::new();
        };
        let origin = self.bounds.origin;
        let width = self.bounds.size.width;
        let mut quads = Vec::new();
        let mut top = start.y;
        while top <= end.y {
            let left = if top == start.y { start.x } else { px(0.) };
            let right = if top == end.y { end.x } else { width };
            quads.push(fill(
                Bounds::from_corners(
                    origin + point(left, top),
                    origin + point(right, top + self.line_height),
                ),
                color,
            ));
            top += self.line_height;
        }
        quads
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        self.document.text().get(range).map(str::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selection),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16.as_ref());
        let kind = if range.is_empty() {
            EditKind::Typing
        } else {
            EditKind::Other
        };
        self.marked = None;
        self.edit(range, text, kind, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16.as_ref());
        self.edit(range.clone(), text, EditKind::Typing, cx);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        if let Some(selected) = new_selected_range_utf16 {
            let selected = self.range_from_utf16(&selected);
            self.selection = range.start + selected.start..range.start + selected.end;
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let painted = self.painted.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let start = painted.position_for(range.start)?;
        let origin = painted.bounds.origin;
        Some(self.zoomed(Bounds::new(
            origin + start,
            size(px(1.), painted.line_height),
        )))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_for_mouse(point);
        Some(self.offset_to_utf16(offset))
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        div()
            .id("editor")
            .key_context("Editor")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_to_line_start))
            .on_action(cx.listener(Self::move_to_line_end))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_to_line_start))
            .on_action(cx.listener(Self::select_to_line_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::toggle_ghost))
            .on_action(cx.listener(Self::open_alternatives))
            .on_action(cx.listener(Self::ai_alternatives))
            .on_action(cx.listener(Self::stash))
            .on_action(cx.listener(Self::close_menu))
            .on_action(cx.listener(Self::delete_active_variant))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
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

struct EditorElement {
    editor: Entity<Editor>,
}

struct EditorPrepaint {
    lines: Vec<PaintedLine>,
    /// Each playing effect, its progress, and the rows of its span.
    effects: Vec<(Particles, f32, Vec<Bounds<Pixels>>)>,
    hover: Vec<PaintQuad>,
    lab_marks: Vec<PaintQuad>,
    selection: Vec<PaintQuad>,
    marker_quads: Vec<PaintQuad>,
    underlines: Vec<PenStroke>,
    cursor: Option<PaintQuad>,
    zoom: focus_zoom::Frame,
}

impl EditorElement {
    /// The effects that play now, each with its progress and the rows of its span. Ended
    /// effects drop; while one plays or a Variant fades in, the next frame comes (FR-78,
    /// FR-79, FR-81).
    fn effect_frames(
        &self,
        painted: &PaintedText,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<(Particles, f32, Vec<Bounds<Pixels>>)> {
        let now = Instant::now();
        let effects = painted.effect_frames(&self.editor.read(cx).effects, now);
        self.editor.update(cx, |editor, _| {
            editor
                .effects
                .retain(|playing| playing.progress(now).is_some());
        });
        let fading = self.editor.read(cx).fading_in(cx).is_some();
        if fading || !effects.is_empty() {
            window.request_animation_frame();
        }
        effects
    }

    /// Text runs and the heading lines with their font size. The title line is bold in the
    /// title color (FR-5) unless it is a Markdown heading, which uses the heading style.
    /// Ghost text dims (FR-37), Trim cuts fade (FR-55), and so does text outside the bright
    /// paragraph while the Alternatives panel is open (FR-27). Slot underlines are pen
    /// strokes, not text runs.
    fn runs(&self, window: &Window, cx: &App) -> (Vec<TextRun>, Vec<(usize, Pixels)>) {
        let editor = self.editor.read(cx);
        let theme = cx.global::<Theme>();
        let style = window.text_style();
        let shown = editor.shown_document();
        let text = shown.text();
        let markdown = shown.markdown_styles();
        let headings = heading_lines(text, &markdown);
        let [large, medium, small] = theme.font_size_heading;
        let sized_lines = headings
            .iter()
            .map(|&(start, level)| {
                let size = match level {
                    MarkdownStyle::Heading1 => large,
                    MarkdownStyle::Heading2 => medium,
                    MarkdownStyle::Heading3
                    | MarkdownStyle::Bold
                    | MarkdownStyle::Italic
                    | MarkdownStyle::Code
                    | MarkdownStyle::Quote
                    | MarkdownStyle::ListMarker => small,
                };
                (start, size)
            })
            .collect();
        let title_is_heading = headings.first().is_some_and(|&(start, _)| start == 0);
        let title_end = if title_is_heading {
            0
        } else {
            shown.title().map_or(0, str::len)
        };
        let underlined: Vec<Range<usize>> = shown
            .slots()
            .into_iter()
            .filter(|slot| slot.level != Level::Paragraph)
            .map(|slot| slot.range)
            .collect();
        let runs: Vec<TextRun> = spans(
            text.len(),
            title_end,
            &underlined,
            &shown.ghost_ranges(),
            &editor.trim_fades(),
            &markdown,
            editor.bright_paragraph().as_ref(),
        )
        .into_iter()
        .map(|span| {
            let mut font = style.font();
            let mut color = theme.text;
            if span.title {
                font.weight = FontWeight::BOLD;
                color = theme.text_title;
            }
            let (font, color, background) = markdown_look(&span.markdown, theme, font, color);
            let opacity = span.opacity(theme);
            TextRun {
                len: span.range.len(),
                font,
                color: color.opacity(opacity),
                background_color: background.map(|background| background.opacity(opacity)),
                underline: None,
                strikethrough: None,
            }
        })
        .collect();
        let fading = editor
            .fading_in(cx)
            .and_then(|(id, opacity)| editor.document.slot(id).map(|slot| (slot.range, opacity)));
        let runs = match fading {
            Some((range, opacity)) => faded_runs(&runs, &range, opacity),
            None => runs,
        };
        if runs.is_empty() {
            let empty = TextRun {
                len: 0,
                font: style.font(),
                color: theme.text,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            return (vec![empty], sized_lines);
        }
        (runs, sized_lines)
    }
}

/// Font, color, and background of a run after the Markdown styles over it, outer first.
fn markdown_look(
    styles: &[MarkdownStyle],
    theme: &Theme,
    mut font: Font,
    mut color: Hsla,
) -> (Font, Hsla, Option<Hsla>) {
    let mut background = None;
    for style in styles {
        match style {
            MarkdownStyle::Heading1 | MarkdownStyle::Heading2 | MarkdownStyle::Heading3 => {
                font.weight = FontWeight::BOLD;
                color = theme.text_heading;
            }
            MarkdownStyle::Bold => font.weight = FontWeight::BOLD,
            MarkdownStyle::Italic => font.style = FontStyle::Italic,
            MarkdownStyle::Code => background = Some(theme.code_background),
            MarkdownStyle::Quote => color = theme.text_quote,
            MarkdownStyle::ListMarker => color = theme.list_marker,
        }
    }
    (font, color, background)
}

/// The start of each line that holds a Markdown heading, with the heading style.
fn heading_lines(text: &str, markdown: &[MarkdownSpan]) -> Vec<(usize, MarkdownStyle)> {
    markdown
        .iter()
        .filter(|span| {
            matches!(
                span.style,
                MarkdownStyle::Heading1 | MarkdownStyle::Heading2 | MarkdownStyle::Heading3
            )
        })
        .map(|span| {
            let before = text.get(..span.range.start).unwrap_or_default();
            let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
            (line_start, span.style)
        })
        .collect()
}

/// Shapes `text` into wrapped lines, each with its start offset and top.
pub(crate) fn shape(
    text: &SharedString,
    runs: &[TextRun],
    wrap_width: Option<Pixels>,
    line_height: Pixels,
    window: &Window,
) -> Vec<PaintedLine> {
    shape_sized(text, runs, &[], wrap_width, line_height, window)
}

/// Like [`shape`], but each line that starts at an offset in `sized_lines` takes that font
/// size in place of the body size. All lines keep `line_height`.
fn shape_sized(
    text: &SharedString,
    runs: &[TextRun],
    sized_lines: &[(usize, Pixels)],
    wrap_width: Option<Pixels>,
    line_height: Pixels,
    window: &Window,
) -> Vec<PaintedLine> {
    let body_size = window.text_style().font_size.to_pixels(window.rem_size());
    let mut start = 0;
    let mut top = px(0.);
    let mut lines = Vec::new();
    for line_text in text.split('\n') {
        let end = start + line_text.len();
        let font_size = sized_lines
            .iter()
            .find(|(at, _)| *at == start)
            .map_or(body_size, |&(_, size)| size);
        let Ok(shaped) = window.text_system().shape_text(
            SharedString::from(line_text.to_owned()),
            font_size,
            &runs_within(runs, &(start..end)),
            wrap_width,
            None,
        ) else {
            return Vec::new();
        };
        for line in shaped {
            let height = line.size(line_height).height;
            lines.push(PaintedLine { start, top, line });
            top += height;
        }
        start = end + 1;
    }
    lines
}

/// `runs` (which cover the text from offset 0) with the part inside byte `range` at
/// `opacity`.
fn faded_runs(runs: &[TextRun], range: &Range<usize>, opacity: f32) -> Vec<TextRun> {
    let mut at = 0;
    let mut faded = Vec::with_capacity(runs.len() + 2);
    for run in runs {
        let start = at;
        at += run.len;
        let inside = start.max(range.start)..at.min(range.end);
        if inside.is_empty() {
            faded.push(run.clone());
            continue;
        }
        let parts = [
            (start..inside.start, 1.),
            (inside.clone(), opacity),
            (inside.end..at, 1.),
        ];
        faded.extend(parts.into_iter().filter(|(part, _)| !part.is_empty()).map(
            |(part, factor)| TextRun {
                len: part.len(),
                color: run.color.opacity(factor),
                background_color: run.background_color.map(|color| color.opacity(factor)),
                ..run.clone()
            },
        ));
    }
    faded
}

/// The parts of `runs` (which cover the text from offset 0) inside byte `range`.
fn runs_within(runs: &[TextRun], range: &Range<usize>) -> Vec<TextRun> {
    let mut at = 0;
    runs.iter()
        .filter_map(|run| {
            let run_start = at;
            at += run.len;
            let len = at.min(range.end).saturating_sub(run_start.max(range.start));
            (len > 0).then(|| TextRun { len, ..run.clone() })
        })
        .collect()
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let text: SharedString = self
            .editor
            .read(cx)
            .shown_document()
            .text()
            .to_owned()
            .into();
        let (runs, sized_lines) = self.runs(window, cx);
        let line_height = window.line_height();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, _| {
                let wrap_width = known.width.or(match available.width {
                    gpui::AvailableSpace::Definite(width) => Some(width),
                    gpui::AvailableSpace::MinContent | gpui::AvailableSpace::MaxContent => None,
                });
                let lines =
                    shape_sized(&text, &runs, &sized_lines, wrap_width, line_height, window);
                let height = lines.last().map_or(line_height, |last| {
                    last.top + last.line.size(line_height).height
                });
                Size {
                    width: wrap_width.unwrap_or_default(),
                    height,
                }
            });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let text: SharedString = self
            .editor
            .read(cx)
            .shown_document()
            .text()
            .to_owned()
            .into();
        let (runs, sized_lines) = self.runs(window, cx);
        let line_height = window.line_height();
        let lines = shape_sized(
            &text,
            &runs,
            &sized_lines,
            Some(bounds.size.width),
            line_height,
            window,
        );
        let painted = PaintedText {
            bounds,
            line_height,
            lines,
        };
        let theme = cx.global::<Theme>().clone();
        let editor = self.editor.read(cx);
        let shown = editor.shown_document();
        let selection =
            painted.selection_quads(&editor.to_shown(&editor.selection), theme.selection);
        let lab_marks = lab_mark_quads(&painted, editor, &theme);
        let slots = shown.slots();
        let hover = editor
            .hovered
            .and_then(|id| slots.iter().find(|slot| slot.id == id))
            .map_or_default(|slot| {
                painted.hover_quads(&slot.range, slot.level, theme.slot_hover_background)
            });
        let mut markers = Vec::new();
        let mut marker_quads = Vec::new();
        for (index, slot) in slots.iter().enumerate() {
            let marker = Marker {
                slot: slot.id,
                kind: match slot.level {
                    Level::Word | Level::Sentence => MarkerKind::Underline,
                    Level::Paragraph => MarkerKind::MarginLine,
                },
                dots: slot.variants.len(),
                active: slot.active,
            };
            let depth = slots
                .iter()
                .take(index)
                .filter(|outer| {
                    outer.level == Level::Paragraph
                        && outer.range.start <= slot.range.start
                        && slot.range.end <= outer.range.end
                })
                .count();
            marker_quads.extend(painted.marker_quads(&slot.range, &marker, depth, &theme));
            markers.push(marker);
        }
        let underlines = pen_strokes(&painted, editor, &slots, &theme);
        let cursor = editor
            .selection
            .is_empty()
            .then(|| {
                painted.position_for(editor.to_shown(&(editor.cursor()..editor.cursor())).start)
            })
            .flatten()
            .map(|position| {
                fill(
                    Bounds::new(bounds.origin + position, size(px(2.), line_height)),
                    theme.accent,
                )
            });
        if editor.autoscroll {
            scroll_into_view(
                &editor.scroll_handle,
                cursor.as_ref(),
                theme.edge_fade_height,
            );
            window.request_animation_frame();
        }
        self.editor.update(cx, |editor, _| {
            editor.autoscroll = false;
            editor.markers = markers;
            editor.underlines.clone_from(&underlines);
        });
        EditorPrepaint {
            effects: self.effect_frames(&painted, window, cx),
            zoom: self.focus_zoom(&painted, &runs, window, cx),
            lines: painted.lines,
            hover,
            lab_marks,
            selection,
            marker_quads,
            underlines,
            cursor,
        }
        .zoomed()
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for quad in prepaint
            .hover
            .drain(..)
            .chain(prepaint.lab_marks.drain(..))
            .chain(prepaint.selection.drain(..))
            .chain(prepaint.marker_quads.drain(..))
        {
            window.paint_quad(quad);
        }
        let line_height = window.line_height();
        let normal = prepaint.lines.iter().enumerate().filter(|(index, _)| {
            !prepaint
                .zoom
                .lines
                .iter()
                .any(|zoomed| zoomed.line == *index)
        });
        for (_, painted) in normal {
            let origin = bounds.origin + point(px(0.), painted.top);
            // A failed glyph paint leaves that line blank; the next frame tries again.
            painted
                .line
                .paint(origin, line_height, gpui::TextAlign::Left, None, window, cx)
                .ok();
        }
        for zoomed in &prepaint.zoom.lines {
            zoomed.paint(window);
        }
        let background = cx.global::<Theme>().background;
        for (playing, progress, rows) in prepaint.effects.drain(..) {
            playing.paint(&rows, line_height, progress, background, window);
        }
        for stroke in &prepaint.underlines {
            if let Some(path) = stroke.path() {
                window.paint_path(path, stroke.color);
            }
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        let lines = std::mem::take(&mut prepaint.lines);
        let zooms = std::mem::take(&mut prepaint.zoom.zooms);
        self.editor.update(cx, |editor, _| {
            editor.zooms = zooms;
            editor.painted = Some(PaintedText {
                bounds,
                line_height,
                lines,
            });
        });
    }
}

/// A stretch of the text with one style.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one independent style layer of the span"
)]
#[derive(Debug, PartialEq, Eq)]
struct Span {
    range: Range<usize>,
    title: bool,
    underlined: bool,
    ghost: bool,
    /// The Markdown styles over it, outer first.
    markdown: Vec<MarkdownStyle>,
    /// Outside the bright paragraph while the Alternatives panel is open (FR-27).
    dimmed: bool,
    /// In a cut of the Trim proposal (FR-55).
    faded: bool,
}

/// Splits text of `len` bytes at the title end, at each underlined, Ghost, Trim cut, and Markdown
/// range edge, and at the edges of the `bright` range, outside of which text is dimmed.
fn spans(
    len: usize,
    title_end: usize,
    underlined: &[Range<usize>],
    ghosts: &[Range<usize>],
    cuts: &[Range<usize>],
    markdown: &[MarkdownSpan],
    bright: Option<&Range<usize>>,
) -> Vec<Span> {
    let mut edges = vec![0, title_end, len];
    edges.extend(bright.iter().flat_map(|range| [range.start, range.end]));
    edges.extend(
        underlined
            .iter()
            .chain(ghosts)
            .chain(cuts)
            .chain(markdown.iter().map(|span| &span.range))
            .flat_map(|range| [range.start, range.end]),
    );
    edges.sort_unstable();
    edges.dedup();
    let within = |ranges: &[Range<usize>], start: usize, end: usize| {
        ranges
            .iter()
            .any(|range| range.start <= start && end <= range.end)
    };
    edges
        .iter()
        .zip(edges.iter().skip(1))
        .map(|(&start, &end)| Span {
            range: start..end,
            title: end <= title_end,
            underlined: within(underlined, start, end),
            ghost: within(ghosts, start, end),
            markdown: markdown
                .iter()
                .filter(|span| span.range.start <= start && end <= span.range.end)
                .map(|span| span.style)
                .collect(),
            faded: within(cuts, start, end),
            dimmed: bright.is_some_and(|bright| !within(std::slice::from_ref(bright), start, end)),
        })
        .collect()
}

impl Span {
    /// Ghost, dim, and Trim fade opacity of the text and its underline.
    fn opacity(&self, theme: &Theme) -> f32 {
        let ghost = if self.ghost { theme.ghost_opacity } else { 1. };
        let dim = if self.dimmed {
            theme.panel_open_dim_opacity
        } else {
            1.
        };
        let fade = if self.faded {
            theme.trim_fade_opacity
        } else {
            1.
        };
        ghost * dim * fade
    }
}

/// One row of a Slot underline in one opacity, drawn like an editor's pen (issue #28).
#[derive(Debug, Clone)]
struct PenStroke {
    slot: SlotId,
    /// Window coordinates.
    centre: Vec<Point<Pixels>>,
    thickness: Vec<Pixels>,
    color: Hsla,
}

impl PenStroke {
    /// The stroke outline as a filled path; `None` when it does not tessellate.
    fn path(&self) -> Option<Path<Pixels>> {
        let half = |thickness: &Pixels| point(px(0.), *thickness / 2.);
        let top = self
            .centre
            .iter()
            .zip(&self.thickness)
            .map(|(at, thickness)| *at - half(thickness));
        let bottom = self
            .centre
            .iter()
            .zip(&self.thickness)
            .rev()
            .map(|(at, thickness)| *at + half(thickness));
        let outline: Vec<Point<Pixels>> = top.chain(bottom).collect();
        let mut builder = PathBuilder::fill();
        builder.add_polygon(&outline, true);
        builder.build().ok()
    }
}

/// A row of text under a pen stroke (local coordinates).
struct PenRow {
    top: Pixels,
    left: Pixels,
    right: Pixels,
}

/// The Lab marks and the Trim cut being walked through, as highlight quads.
fn lab_mark_quads(painted: &PaintedText, editor: &Editor, theme: &Theme) -> Vec<PaintQuad> {
    let walked = editor.document.trim_proposal().and_then(|proposal| {
        proposal
            .current()
            .and_then(|current| proposal.cuts().get(current))
    });
    editor
        .document
        .lab_marks()
        .iter()
        .map(|mark| &mark.range)
        .chain(walked.map(|cut| &cut.range))
        .flat_map(|range| painted.selection_quads(range, theme.lab_mark))
        .collect()
}

/// The pen strokes under the Word and Sentence Slots: one per row and opacity. Each span
/// belongs to the innermost Slot over it, which gives its stroke its seed.
fn pen_strokes(
    painted: &PaintedText,
    editor: &Editor,
    slots: &[Slot],
    theme: &Theme,
) -> Vec<PenStroke> {
    let text = editor.shown_document().text();
    editor
        .underline_spans()
        .into_iter()
        .filter(|span| span.underlined)
        .filter_map(|span| {
            let owner = slots
                .iter()
                .filter(|slot| {
                    slot.level != Level::Paragraph
                        && slot.range.start <= span.range.start
                        && span.range.end <= slot.range.end
                })
                .min_by_key(|slot| slot.range.len())?;
            let color = theme.slot_underline.opacity(span.opacity(theme));
            Some(painted.pen_strokes(text, &span.range, owner, color))
        })
        .flatten()
        .collect()
}

/// Offset from a straight line and thickness of a pen stroke `along` px into its Slot. Two
/// slow sines with phases from `seed`: a hand-drawn line, not a zigzag.
fn pen(seed: u64, along: f32) -> (f32, f32) {
    let phase = |shift: u32| f32::from(u16::try_from((seed >> shift) & 0x3ff).unwrap_or(0)) / 1024.;
    let wave = |wavelength: f32, shift: u32| ((along / wavelength + phase(shift)) * TAU).sin();
    let offset =
        PEN_WOBBLE * (0.65 * wave(PEN_WAVELENGTH, 0) + 0.35 * wave(PEN_WAVELENGTH * 0.37, 10));
    let thickness = PEN_THICKNESS + PEN_THICKNESS_WOBBLE * wave(PEN_WAVELENGTH * 0.61, 20);
    (offset, thickness)
}

/// `count` as a factor for pixel math; counts past `u16::MAX` stop there.
fn as_factor(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

/// Where `offset` in `old` lands in `new`, when one span of `old` changed into `new`.
/// An offset inside the changed span goes to the end of the new span.
pub(crate) fn follow_change(offset: usize, old: &str, new: &str) -> usize {
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((index, _), _)| index);
    let suffix = old
        .get(prefix..)
        .unwrap_or_default()
        .chars()
        .rev()
        .zip(new.get(prefix..).unwrap_or_default().chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    if offset <= prefix {
        return offset;
    }
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    if offset >= old_end {
        return offset - old_end + new_end;
    }
    new_end
}

/// Scrolls the least amount that keeps the cursor out of the edge fade.
pub(crate) fn scroll_into_view(
    scroll_handle: &ScrollHandle,
    cursor: Option<&PaintQuad>,
    margin: Pixels,
) {
    let Some(cursor) = cursor else {
        return;
    };
    let viewport = scroll_handle.bounds();
    let mut offset = scroll_handle.offset();
    let above = viewport.top() + margin - cursor.bounds.top();
    let below = cursor.bounds.bottom() - (viewport.bottom() - margin);
    if above > px(0.) {
        offset.y += above;
    }
    if below > px(0.) {
        offset.y -= below;
    }
    offset.y = offset.y.min(px(0.));
    scroll_handle.set_offset(offset);
}

#[cfg(test)]
mod tests {
    use ka_teb_core::{MarkdownSpan, MarkdownStyle};

    use gpui::{TextRun, font, hsla};

    use super::{Span, faded_runs, follow_change, heading_lines, pen, spans};

    #[test]
    fn a_fade_splits_the_runs_at_its_range_and_dims_only_inside() {
        let run = |len| TextRun {
            len,
            font: font("IBM Plex Mono"),
            color: hsla(0., 0., 1., 1.),
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let faded = faded_runs(&[run(4), run(6)], &(2..7), 0.5);

        let shape: Vec<(usize, f32)> = faded.iter().map(|run| (run.len, run.color.a)).collect();
        assert_eq!(shape, [(2, 1.), (2, 0.5), (3, 0.5), (3, 1.)]);
    }

    fn samples(seed: u64) -> Vec<(f32, f32)> {
        (0..400u16)
            .map(|along| pen(seed, f32::from(along)))
            .collect()
    }

    #[test]
    fn the_pen_line_wobbles_about_half_to_one_pixel() {
        let wobble = samples(7)
            .iter()
            .map(|(offset, _)| offset.abs())
            .fold(0., f32::max);
        assert!((0.5..=1.).contains(&wobble), "wobble {wobble}");
    }

    #[test]
    fn the_pen_line_thickness_changes_a_little() {
        let thickness: Vec<f32> = samples(7).iter().map(|(_, thickness)| *thickness).collect();
        let thinnest = thickness.iter().copied().fold(f32::MAX, f32::min);
        let thickest = thickness.iter().copied().fold(0., f32::max);
        assert!(thinnest >= 0.7 && thickest <= 1.3, "{thinnest}..{thickest}");
        assert!(thickest - thinnest > 0.2, "{thinnest}..{thickest}");
    }

    #[test]
    fn slots_with_other_seeds_draw_other_lines() {
        assert_ne!(samples(7), samples(0x5_0505_0505));
    }

    #[test]
    fn ghost_and_slot_ranges_split_the_text_into_styled_spans() {
        let span = |range, title, underlined, ghost| Span {
            range,
            title,
            underlined,
            ghost,
            markdown: Vec::new(),
            dimmed: false,
            faded: false,
        };
        assert_eq!(
            spans(
                27,
                5,
                std::slice::from_ref(&(14..21)),
                std::slice::from_ref(&(6..13)),
                &[],
                &[],
                None
            ),
            vec![
                span(0..5, true, false, false),
                span(5..6, false, false, false),
                span(6..13, false, false, true),
                span(13..14, false, false, false),
                span(14..21, false, true, false),
                span(21..27, false, false, false),
            ]
        );
    }

    #[test]
    fn markdown_styles_split_spans_and_nest() {
        let markdown = |range, style| MarkdownSpan { range, style };
        let span = |range, markdown| Span {
            range,
            title: false,
            underlined: false,
            ghost: false,
            markdown,
            dimmed: false,
            faded: false,
        };
        assert_eq!(
            spans(
                12,
                0,
                &[],
                &[],
                &[],
                &[
                    markdown(0..12, MarkdownStyle::Bold),
                    markdown(2..5, MarkdownStyle::Italic)
                ],
                None
            ),
            vec![
                span(0..2, vec![MarkdownStyle::Bold]),
                span(2..5, vec![MarkdownStyle::Bold, MarkdownStyle::Italic]),
                span(5..12, vec![MarkdownStyle::Bold]),
            ]
        );
    }

    #[test]
    fn heading_lines_start_at_the_line_of_each_heading() {
        let text = "# Top\nbody\n> ## Quoted\n### Low";
        let styles = ka_teb_core::Document::new(text).markdown_styles();
        assert_eq!(
            heading_lines(text, &styles),
            vec![
                (0, MarkdownStyle::Heading1),
                (11, MarkdownStyle::Heading2),
                (23, MarkdownStyle::Heading3)
            ]
        );
    }

    #[test]
    fn text_outside_the_bright_range_is_dimmed() {
        let dimmed: Vec<(std::ops::Range<usize>, bool)> =
            spans(30, 5, &[], &[], &[], &[], Some(&(6..18)))
                .into_iter()
                .map(|span| (span.range, span.dimmed))
                .collect();
        assert_eq!(
            dimmed,
            vec![(0..5, true), (5..6, true), (6..18, false), (18..30, true)]
        );
    }

    #[test]
    fn trim_cuts_fade_apart_from_ghost_ranges() {
        let faded: Vec<(std::ops::Range<usize>, bool, bool)> = spans(
            20,
            0,
            &[],
            std::slice::from_ref(&(8..12)),
            &[2..5, 10..15],
            &[],
            None,
        )
        .into_iter()
        .map(|span| (span.range, span.ghost, span.faded))
        .collect();
        assert_eq!(
            faded,
            vec![
                (0..2, false, false),
                (2..5, false, true),
                (5..8, false, false),
                (8..10, true, false),
                (10..12, true, true),
                (12..15, false, true),
                (15..20, false, false),
            ]
        );
    }

    #[test]
    fn an_offset_after_a_changed_span_moves_by_its_change_in_length() {
        let old = "It is an eraser here.";
        let new = "It is a thumbtack here.";
        assert_eq!(follow_change(3, old, new), 3);
        assert_eq!(follow_change(21, old, new), 23);
        assert_eq!(follow_change(10, old, new), 17);
    }

    #[test]
    fn a_shared_suffix_does_not_reach_into_the_shared_prefix() {
        assert_eq!(follow_change(1, "aa", "a"), 1);
        assert_eq!(follow_change(2, "aa", "a"), 1);
    }
}
