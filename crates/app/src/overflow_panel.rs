//! The Overflow panel: right side, a free text area over the Document's
//! Overflow. Edits go through the [`Editor`], so they share its undo history.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, MouseButton, MouseDownEvent, PaintQuad, Pixels, Point, Render,
    ScrollHandle, SharedString, Size, Style, Subscription, TextRun, UTF16Selection, Window, div,
    fill, point, prelude::*, px, relative, size,
};
use ka_teb_core::{EditKind, follow_change};
use unicode_segmentation::UnicodeSegmentation;

use crate::actions::{
    Backspace, Copy, Cut, Delete, LeaveOverflow, MoveDown, MoveLeft, MoveRight, MoveToLineEnd,
    MoveToLineStart, MoveUp, Newline, Paste, Redo, SelectAll, SelectDown, SelectLeft, SelectRight,
    SelectToLineEnd, SelectToLineStart, SelectUp, ToggleOverflow, Undo, UseOverflowBlock,
};
use crate::editor::{PaintedText, scroll_into_view, shape};
use crate::{Editor, Theme};

/// The value a drag from the Overflow text carries: the block at the panel caret.
pub(crate) struct OverflowBlockDrag;

/// The Overflow panel: the "Overflow" title, the free text area, and the footer hint.
pub struct OverflowPanel {
    editor: Entity<Editor>,
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    painted: Option<PaintedText>,
    autoscroll: bool,
    _editor_changes: Subscription,
}

impl OverflowPanel {
    pub(crate) fn new(editor: Entity<Editor>, cx: &mut Context<Self>) -> Self {
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        Self {
            editor,
            selection: 0..0,
            reversed: false,
            marked: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            painted: None,
            autoscroll: false,
            _editor_changes: editor_changes,
        }
    }

    /// Byte range of the selection in the Overflow text; empty at the caret.
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    fn text(&self, cx: &App) -> String {
        self.editor.read(cx).document().overflow().text().to_owned()
    }

    pub(crate) fn caret(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    /// Keeps the selection inside the text after a change from outside the panel.
    fn clamp(&mut self, cx: &App) {
        let text = self.text(cx);
        let fit = |offset: usize| floor_boundary(&text, offset);
        self.selection = fit(self.selection.start)..fit(self.selection.end);
        self.marked = self
            .marked
            .take()
            .map(|marked| fit(marked.start)..fit(marked.end));
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selection = offset..offset;
        self.reversed = false;
        self.after_move(cx);
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
        self.after_move(cx);
    }

    fn after_move(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, _| editor.close_history());
        self.autoscroll = true;
        cx.notify();
    }

    fn edit(&mut self, range: Range<usize>, text: &str, kind: EditKind, cx: &mut Context<Self>) {
        let done = self.editor.update(cx, |editor, cx| {
            editor.edit_overflow(range.clone(), text, kind, cx)
        });
        if !done {
            return;
        }
        let caret = range.start + text.len();
        self.selection = caret..caret;
        self.reversed = false;
        self.autoscroll = true;
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize, cx: &App) -> usize {
        self.text(cx)
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize, cx: &App) -> usize {
        let text = self.text(cx);
        text.grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(text.len())
    }

    /// The offset `rows` visual rows above (negative) or below the caret.
    fn offset_rows_away(&self, rows: f32, cx: &App) -> usize {
        let caret = self.caret();
        let Some(painted) = self.painted.as_ref() else {
            return caret;
        };
        let Some(position) = painted.position_for(caret) else {
            return caret;
        };
        let y = position.y + painted.line_height * (rows + 0.5);
        painted.offset_for_local(point(position.x, y), self.text(cx).len())
    }

    /// The offset at the start (`x` = 0) or end (`x` = MAX) of the caret's visual row.
    fn offset_in_row(&self, x: Pixels, cx: &App) -> usize {
        let caret = self.caret();
        let Some(painted) = self.painted.as_ref() else {
            return caret;
        };
        let Some(position) = painted.position_for(caret) else {
            return caret;
        };
        let y = position.y + painted.line_height * 0.5;
        painted.offset_for_local(point(x, y), self.text(cx).len())
    }

    fn offset_for_mouse(&self, position: Point<Pixels>, cx: &App) -> usize {
        let Some(painted) = self.painted.as_ref() else {
            return self.caret();
        };
        painted.offset_for_local(position - painted.bounds.origin, self.text(cx).len())
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        let offset = if self.selection.is_empty() {
            self.previous_boundary(self.caret(), cx)
        } else {
            self.selection.start
        };
        self.move_to(offset, cx);
    }

    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        let offset = if self.selection.is_empty() {
            self.next_boundary(self.caret(), cx)
        } else {
            self.selection.end
        };
        self.move_to(offset, cx);
    }

    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.offset_rows_away(-1., cx), cx);
    }

    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.offset_rows_away(1., cx), cx);
    }

    fn move_to_line_start(&mut self, _: &MoveToLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.offset_in_row(px(0.), cx), cx);
    }

    fn move_to_line_end(&mut self, _: &MoveToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.offset_in_row(Pixels::MAX, cx), cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        self.select_to(self.previous_boundary(self.caret(), cx), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        self.select_to(self.next_boundary(self.caret(), cx), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.offset_rows_away(-1., cx), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.offset_rows_away(1., cx), cx);
    }

    fn select_to_line_start(
        &mut self,
        _: &SelectToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.offset_in_row(px(0.), cx), cx);
    }

    fn select_to_line_end(&mut self, _: &SelectToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.offset_in_row(Pixels::MAX, cx), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.text(cx).len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        if !self.selection.is_empty() {
            self.edit(self.selection.clone(), "", EditKind::Other, cx);
            return;
        }
        let caret = self.caret();
        let start = self.previous_boundary(caret, cx);
        self.edit(start..caret, "", EditKind::OverflowDeleting, cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        if !self.selection.is_empty() {
            self.edit(self.selection.clone(), "", EditKind::Other, cx);
            return;
        }
        let caret = self.caret();
        let end = self.next_boundary(caret, cx);
        self.edit(caret..end, "", EditKind::OverflowDeleting, cx);
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        self.edit(self.selection.clone(), "\n", EditKind::Other, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        let text = self.text(cx);
        let Some(selected) = text.get(self.selection.clone()) else {
            return;
        };
        if !selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_owned()));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        self.edit(self.selection.clone(), "", EditKind::Other, cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.edit(self.selection.clone(), &text, EditKind::Other, cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let old = self.text(cx);
        self.editor.update(cx, Editor::undo_step);
        self.follow(&old, cx);
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let old = self.text(cx);
        self.editor.update(cx, Editor::redo_step);
        self.follow(&old, cx);
    }

    /// Moves the selection along with a change of the Overflow text from `old`.
    fn follow(&mut self, old: &str, cx: &mut Context<Self>) {
        let new = self.text(cx);
        if new == old {
            return;
        }
        self.selection = follow_change(self.selection.start, old, &new)
            ..follow_change(self.selection.end, old, &new);
        self.marked = None;
        self.autoscroll = true;
        cx.notify();
    }

    /// Ctrl+Return puts the block at the caret into the Document.
    fn use_block(&mut self, _: &UseOverflowBlock, _: &mut Window, cx: &mut Context<Self>) {
        self.clamp(cx);
        let caret = self.caret();
        self.editor
            .update(cx, |editor, cx| editor.use_overflow_block(caret, cx));
    }

    /// Esc gives the keyboard back to the Document; Overflow stays open.
    fn leave(&mut self, _: &LeaveOverflow, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.editor.focus_handle(cx));
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.clamp(cx);
        let offset = self.offset_for_mouse(event.position, cx);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    /// The text of the Overflow block at the caret, for the drag preview.
    fn block_at_caret(&self, cx: &App) -> String {
        let text = self.text(cx);
        let caret = self.caret();
        self.editor
            .read(cx)
            .document()
            .overflow()
            .blocks()
            .into_iter()
            .find(|block| block.start <= caret && caret <= block.end)
            .and_then(|block| text.get(block))
            .unwrap_or_default()
            .to_owned()
    }

    fn range_to_utf16(&self, range: &Range<usize>, cx: &App) -> Range<usize> {
        let text = self.text(cx);
        to_utf16(&text, range.start)..to_utf16(&text, range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>, cx: &App) -> Range<usize> {
        let text = self.text(cx);
        from_utf16(&text, range.start)..from_utf16(&text, range.end)
    }

    fn input_range(&self, range_utf16: Option<&Range<usize>>, cx: &App) -> Range<usize> {
        range_utf16
            .map(|range| self.range_from_utf16(range, cx))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone())
    }
}

impl EntityInputHandler for OverflowPanel {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16, cx);
        actual_range.replace(self.range_to_utf16(&range, cx));
        self.text(cx).get(range).map(str::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.clamp(cx);
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selection, cx),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|range| self.range_to_utf16(range, cx))
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
        self.clamp(cx);
        let range = self.input_range(range_utf16.as_ref(), cx);
        let kind = if range.is_empty() {
            EditKind::OverflowTyping
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
        self.clamp(cx);
        let range = self.input_range(range_utf16.as_ref(), cx);
        self.edit(range.clone(), text, EditKind::OverflowTyping, cx);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        if let Some(selected) = new_selected_range_utf16 {
            let selected = self.range_from_utf16(&selected, cx);
            self.selection = range.start + selected.start..range.start + selected.end;
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16, cx);
        let painted = self.painted.as_ref()?;
        let start = painted.position_for(range.start)?;
        Some(Bounds::new(
            painted.bounds.origin + start,
            size(px(1.), painted.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_for_mouse(point, cx);
        Some(to_utf16(&self.text(cx), offset))
    }
}

impl Focusable for OverflowPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for OverflowPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let panel = cx.entity();
        div()
            .id("overflow-panel")
            .key_context("OverflowPanel")
            .track_focus(&self.focus_handle)
            .occlude()
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(theme.panel_width)
            .flex()
            .flex_col()
            .bg(theme.panel_background)
            .border_l_1()
            .border_color(theme.panel_border)
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
            .on_action(cx.listener(Self::use_block))
            .on_action(cx.listener(Self::leave))
            .child(
                div()
                    .px(px(16.))
                    .pt(px(22.))
                    .pb(px(12.))
                    .text_color(theme.accent)
                    .child("Overflow"),
            )
            .child(
                div()
                    .id("overflow-text")
                    .flex_1()
                    .px(px(16.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .cursor(CursorStyle::IBeam)
                    .text_size(theme.font_size_overflow)
                    .line_height(theme.line_height_overflow)
                    .text_color(theme.text)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                    .on_drag(OverflowBlockDrag, move |_, _, _, cx| {
                        let text = panel.read(cx).block_at_caret(cx);
                        cx.new(|_| DraggedBlock { text: text.into() })
                    })
                    .child(OverflowText { panel: cx.entity() }),
            )
            .child(
                div()
                    .px(px(16.))
                    .pt(px(8.))
                    .pb(px(16.))
                    .text_size(theme.font_size_hint)
                    .text_color(theme.text_muted)
                    .child("ctrl+\u{21b5} or drag into the page to use"),
            )
    }
}

/// What follows the pointer while a block is dragged into the page.
struct DraggedBlock {
    text: SharedString,
}

impl Render for DraggedBlock {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        div()
            .max_w(theme.panel_width)
            .px(px(8.))
            .py(px(4.))
            .bg(theme.elevated_background)
            .rounded(theme.elevated_radius)
            .text_size(theme.font_size_overflow)
            .line_height(theme.line_height_overflow)
            .text_color(theme.text_muted)
            .child(self.text.clone())
    }
}

/// The Overflow text element: wraps, paints the selection and caret, takes text input.
struct OverflowText {
    panel: Entity<OverflowPanel>,
}

struct OverflowPrepaint {
    painted: PaintedText,
    selection: Vec<PaintQuad>,
    caret: Option<PaintQuad>,
}

impl OverflowText {
    fn runs(len: usize, window: &Window, cx: &App) -> Vec<TextRun> {
        vec![TextRun {
            len,
            font: window.text_style().font(),
            color: cx.global::<Theme>().text,
            background_color: None,
            underline: None,
            strikethrough: None,
        }]
    }
}

impl IntoElement for OverflowText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for OverflowText {
    type RequestLayoutState = ();
    type PrepaintState = OverflowPrepaint;

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
        let text: SharedString = self.panel.read(cx).text(cx).into();
        let runs = Self::runs(text.len(), window, cx);
        let line_height = window.line_height();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, _| {
                let wrap_width = known.width.or(match available.width {
                    gpui::AvailableSpace::Definite(width) => Some(width),
                    gpui::AvailableSpace::MinContent | gpui::AvailableSpace::MaxContent => None,
                });
                let lines = shape(&text, &runs, wrap_width, line_height, window);
                let height = PaintedText {
                    bounds: Bounds::default(),
                    line_height,
                    lines,
                }
                .height();
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
        self.panel.update(cx, |panel, cx| panel.clamp(cx));
        let text: SharedString = self.panel.read(cx).text(cx).into();
        let runs = Self::runs(text.len(), window, cx);
        let line_height = window.line_height();
        let painted = PaintedText {
            bounds,
            line_height,
            lines: shape(&text, &runs, Some(bounds.size.width), line_height, window),
        };
        let theme = cx.global::<Theme>().clone();
        let panel = self.panel.read(cx);
        let selection = painted.selection_quads(&panel.selection, theme.selection);
        let caret = panel
            .selection
            .is_empty()
            .then(|| painted.position_for(panel.caret()))
            .flatten()
            .map(|position| {
                fill(
                    Bounds::new(bounds.origin + position, size(px(1.), line_height)),
                    theme.accent,
                )
            });
        if panel.autoscroll {
            scroll_into_view(&panel.scroll_handle, caret.as_ref(), px(0.));
            window.request_animation_frame();
        }
        self.panel.update(cx, |panel, _| panel.autoscroll = false);
        OverflowPrepaint {
            painted,
            selection,
            caret,
        }
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
        let focus_handle = self.panel.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.panel.clone()),
            cx,
        );
        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }
        prepaint.painted.paint(window, cx);
        if focus_handle.is_focused(window)
            && let Some(caret) = prepaint.caret.take()
        {
            window.paint_quad(caret);
        }
        let painted = std::mem::replace(
            &mut prepaint.painted,
            PaintedText {
                bounds,
                line_height: px(0.),
                lines: Vec::new(),
            },
        );
        self.panel
            .update(cx, |panel, _| panel.painted = Some(painted));
    }
}

/// `offset` moved back to a char boundary of `text`, at most its length.
fn floor_boundary(text: &str, offset: usize) -> usize {
    (0..=offset.min(text.len()))
        .rev()
        .find(|&at| text.is_char_boundary(at))
        .unwrap_or(0)
}

fn to_utf16(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(0, |before| before.chars().map(char::len_utf16).sum())
}

fn from_utf16(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    text.char_indices()
        .find_map(|(index, ch)| {
            let found = (utf16 >= offset).then_some(index);
            utf16 += ch.len_utf16();
            found
        })
        .unwrap_or(text.len())
}

/// The Chrome button that opens and closes Overflow: an outline badge, accent when open.
pub(crate) fn overflow_button(open: bool, theme: &Theme) -> impl IntoElement {
    let outline = if open {
        theme.accent_outline
    } else {
        theme.text_muted
    };
    div()
        .id("overflow-button")
        .debug_selector(|| "overflow-button".to_owned())
        .ml_auto()
        .px(px(4.))
        .border_1()
        .border_color(outline)
        .rounded(px(3.))
        .text_size(theme.font_size_hint)
        .text_color(outline)
        .cursor_pointer()
        .tooltip(|_, cx| {
            cx.new(|_| Tooltip {
                text: "Ctrl+Shift+O".into(),
            })
            .into()
        })
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleOverflow), cx))
        .child("XYZ")
}

struct Tooltip {
    text: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        div()
            .px(px(6.))
            .py(px(2.))
            .bg(theme.elevated_background)
            .rounded(theme.elevated_radius)
            .text_size(theme.font_size_hint)
            .text_color(theme.text_muted)
            .child(self.text.clone())
    }
}
