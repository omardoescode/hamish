use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, Render, ScrollHandle, SharedString, Size, Style,
    TextRun, UTF16Selection, Window, WrappedLine, div, fill, point, prelude::*, px, relative, size,
};
use ka_teb_core::Document;
use unicode_segmentation::UnicodeSegmentation;

use crate::Theme;
use crate::actions::{
    Backspace, Copy, Cut, Delete, MoveDown, MoveLeft, MoveRight, MoveToLineEnd, MoveToLineStart,
    MoveUp, Newline, Paste, Redo, SelectAll, SelectDown, SelectLeft, SelectRight, SelectToLineEnd,
    SelectToLineStart, SelectUp, Undo,
};
use crate::edit_history::{EditHistory, EditKind};

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
}

/// The text as last painted, for mouse and vertical cursor moves.
struct PaintedText {
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    lines: Vec<PaintedLine>,
}

struct PaintedLine {
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
        }
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Byte range of the selection; empty at the cursor.
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
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
        let local = position - painted.bounds.origin;
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
        self.move_to(self.offset_rows_away(-1.), cx);
    }

    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.offset_rows_away(1.), cx);
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
        let Some(group) = self.history.undo() else {
            return;
        };
        for _ in 0..group.steps {
            self.document.undo();
        }
        self.selection = group.before;
        self.reversed = false;
        self.autoscroll = true;
        cx.notify();
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.history.redo() else {
            return;
        };
        for _ in 0..group.steps {
            self.document.redo();
        }
        self.selection = group.after;
        self.reversed = false;
        self.autoscroll = true;
        cx.notify();
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.selecting = true;
        let offset = self.offset_for_mouse(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.offset_for_mouse(event.position), cx);
        }
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
    /// Position of `offset`, relative to the text origin.
    fn position_for(&self, offset: usize) -> Option<Point<Pixels>> {
        let painted = self.lines.iter().rev().find(|line| line.start <= offset)?;
        let position = painted
            .line
            .position_for_index(offset - painted.start, self.line_height)?;
        Some(point(position.x, painted.top + position.y))
    }

    /// Offset closest to `local` (relative to the text origin). Above the text is 0,
    /// below it is `len`.
    fn offset_for_local(&self, local: Point<Pixels>, len: usize) -> usize {
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

    /// Quads that cover `range` (window coordinates), one per visual row.
    fn selection_quads(&self, range: &Range<usize>, color: Hsla) -> Vec<PaintQuad> {
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
        Some(Bounds::new(
            origin + start,
            size(px(1.), painted.line_height),
        ))
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
        let theme = *cx.global::<Theme>();
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
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
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
    }
}

struct EditorElement {
    editor: Entity<Editor>,
}

struct EditorPrepaint {
    lines: Vec<PaintedLine>,
    selection: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl EditorElement {
    /// Text runs: the title line bold in the title color (FR-5), the rest in body style.
    fn runs(&self, window: &Window, cx: &App) -> Vec<TextRun> {
        let editor = self.editor.read(cx);
        let theme = cx.global::<Theme>();
        let style = window.text_style();
        let text = editor.document.text();
        let body = TextRun {
            len: text.len(),
            font: style.font(),
            color: theme.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let Some(title) = editor.document.title() else {
            return vec![body];
        };
        let mut title_font = style.font();
        title_font.weight = FontWeight::BOLD;
        vec![
            TextRun {
                len: title.len(),
                font: title_font,
                color: theme.text_title,
                ..body.clone()
            },
            TextRun {
                len: text.len() - title.len(),
                ..body
            },
        ]
    }

    fn shape(
        text: &SharedString,
        runs: &[TextRun],
        wrap_width: Option<Pixels>,
        line_height: Pixels,
        window: &Window,
    ) -> Vec<PaintedLine> {
        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        let Ok(shaped) =
            window
                .text_system()
                .shape_text(text.clone(), font_size, runs, wrap_width, None)
        else {
            return Vec::new();
        };
        let mut start = 0;
        let mut top = px(0.);
        let mut lines = Vec::new();
        for line in shaped {
            let height = line.size(line_height).height;
            let len = line.len();
            lines.push(PaintedLine { start, top, line });
            start += len + 1;
            top += height;
        }
        lines
    }
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
        let text: SharedString = self.editor.read(cx).document.text().to_owned().into();
        let runs = self.runs(window, cx);
        let line_height = window.line_height();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, _| {
                let wrap_width = known.width.or(match available.width {
                    gpui::AvailableSpace::Definite(width) => Some(width),
                    gpui::AvailableSpace::MinContent | gpui::AvailableSpace::MaxContent => None,
                });
                let lines = Self::shape(&text, &runs, wrap_width, line_height, window);
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
        let text: SharedString = self.editor.read(cx).document.text().to_owned().into();
        let runs = self.runs(window, cx);
        let line_height = window.line_height();
        let lines = Self::shape(&text, &runs, Some(bounds.size.width), line_height, window);
        let painted = PaintedText {
            bounds,
            line_height,
            lines,
        };
        let theme = *cx.global::<Theme>();
        let editor = self.editor.read(cx);
        let selection = painted.selection_quads(&editor.selection, theme.selection);
        let cursor = editor
            .selection
            .is_empty()
            .then(|| painted.position_for(editor.cursor()))
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
        self.editor
            .update(cx, |editor, _| editor.autoscroll = false);
        EditorPrepaint {
            lines: painted.lines,
            selection,
            cursor,
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
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }
        let line_height = window.line_height();
        for painted in &prepaint.lines {
            let origin = bounds.origin + point(px(0.), painted.top);
            // A failed glyph paint leaves that line blank; the next frame tries again.
            painted
                .line
                .paint(origin, line_height, gpui::TextAlign::Left, None, window, cx)
                .ok();
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        let lines = std::mem::take(&mut prepaint.lines);
        self.editor.update(cx, |editor, _| {
            editor.painted = Some(PaintedText {
                bounds,
                line_height,
                lines,
            });
        });
    }
}

/// Scrolls the least amount that keeps the cursor out of the edge fade.
fn scroll_into_view(scroll_handle: &ScrollHandle, cursor: Option<&PaintQuad>, margin: Pixels) {
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
