//! The Overflow panel: right side, a free text area over the Document's
//! Overflow, with the same Vim mode as the Document. Edits go through the [`Editor`], so
//! they share its undo history.

use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    MouseButton, MouseDownEvent, PaintQuad, Pixels, Render, ScrollHandle, SharedString, Size,
    Style, Subscription, TextRun, Window, div, prelude::*, px, relative,
};
use hamish_core::{Caret, CaretEdit, Vim, VimCommand, VimKey, VimMode};

use crate::actions::{LeaveOverflow, Redo, ToggleOverflow, Undo, UseOverflowBlock};
use crate::caret::{self, CaretBlink, CaretView, Column, caret_actions, caret_input_handler};
use crate::chrome::elevated_surface;
use crate::editor::{
    PaintedText, VimView, gpui_action, run_text_command, run_vim, shape, vim_key_down, vim_on,
};
use crate::{Editor, Theme};

/// The value a drag from the Overflow text carries: the block at the panel caret.
pub(crate) struct OverflowBlockDrag;

/// The Overflow panel: the "Overflow" title, the free text area, and the footer hint.
pub struct OverflowPanel {
    editor: Entity<Editor>,
    caret: Caret,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    painted: Option<PaintedText>,
    caret_bounds: Option<Bounds<Pixels>>,
    blink: CaretBlink,
    autoscroll: bool,
    column: Option<Column>,
    /// The Vim key state of the Overflow text; it starts again each time Vim mode turns on.
    vim: Vim,
    /// The font family of the text as last painted.
    text_font: SharedString,
    _editor_changes: Subscription,
}

impl OverflowPanel {
    pub(crate) fn new(editor: Entity<Editor>, cx: &mut Context<Self>) -> Self {
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        Self {
            editor,
            caret: Caret::default(),
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            painted: None,
            caret_bounds: None,
            blink: CaretBlink::new(cx),
            autoscroll: false,
            column: None,
            vim: Vim::default(),
            text_font: SharedString::default(),
            _editor_changes: editor_changes,
        }
    }

    /// The font family of the Overflow text as last painted.
    #[must_use]
    pub fn text_font(&self) -> &SharedString {
        &self.text_font
    }

    /// Byte range of the selection in the Overflow text; empty at the caret.
    #[must_use]
    pub fn selection(&self) -> std::ops::Range<usize> {
        self.caret.selection()
    }

    /// The caret as last painted (window coordinates); `None` when none shows.
    #[must_use]
    pub fn caret_bounds(&self) -> Option<Bounds<Pixels>> {
        self.caret_bounds
    }

    fn text(&self, cx: &App) -> String {
        self.read_text(cx, |text, _| text.to_owned())
    }

    /// The caret offset in the Overflow text as it is now.
    pub(crate) fn caret(&self, cx: &App) -> usize {
        self.read_text(cx, |text, caret| caret.clamped(text).offset())
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
        self.caret = self.caret.followed(old, &new);
        self.blink.restart(cx);
        self.autoscroll = true;
        cx.notify();
    }

    /// Ctrl+Return puts the block at the caret into the Document.
    fn use_block(&mut self, _: &UseOverflowBlock, _: &mut Window, cx: &mut Context<Self>) {
        let caret = self.caret(cx);
        self.editor
            .update(cx, |editor, cx| editor.use_overflow_block(caret, cx));
    }

    /// Esc gives the keyboard back to the Document; Overflow stays open. In Vim mode, Esc in
    /// Insert or Visual mode goes to Normal mode and Overflow keeps the keyboard.
    fn leave(&mut self, _: &LeaveOverflow, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self.vim_mode(cx);
        if mode.is_some() {
            run_vim(self, VimKey::Escape, window, cx);
        }
        if matches!(mode, None | Some(VimMode::Normal)) {
            window.focus(&self.editor.focus_handle(cx), cx);
        }
    }

    /// The Vim mode of the Overflow text; `None` when the Vim mode setting is off.
    #[must_use]
    pub fn vim_mode(&self, cx: &App) -> Option<VimMode> {
        vim_on(cx).then(|| self.vim.mode())
    }

    /// The Vim mode and the keys of a command not yet complete, as the Chrome bar shows
    /// them while Overflow has the keyboard; `None` when the Vim mode setting is off.
    #[must_use]
    pub fn vim_status(&self, cx: &App) -> Option<String> {
        vim_on(cx).then(|| self.vim.status())
    }

    /// Runs `step` `times` times; the caret follows, then goes to the start of the selection.
    fn undo_redo(
        &mut self,
        times: usize,
        step: fn(&mut Editor, &mut Context<Editor>),
        cx: &mut Context<Self>,
    ) {
        let old = self.text(cx);
        for _ in 0..times {
            self.editor.update(cx, step);
        }
        self.follow(&old, cx);
        let start = self.caret.selection().start;
        self.select(Caret::at(start), cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        match event.click_count {
            2 => caret::select_word(self, event.position, cx),
            _ if self.press_visual(event.position, event.modifiers.shift, cx) => {}
            _ => caret::press(self, event.position, event.modifiers.shift, cx),
        }
    }

    /// A press at `position` in Visual mode moves the end of the Visual selection there;
    /// `false` outside Visual mode.
    fn press_visual(
        &mut self,
        position: gpui::Point<Pixels>,
        extend: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !vim_on(cx) {
            return false;
        }
        let offset = caret::offset_at(self, position, cx);
        let text = self.text(cx);
        let Some(caret) = self.vim.click(&text, offset, extend) else {
            return false;
        };
        CaretView::select(self, caret, cx);
        true
    }

    /// The text of the Overflow block at the caret, for the drag preview.
    fn block_at_caret(&self, cx: &App) -> String {
        let text = self.text(cx);
        let caret = self.caret(cx);
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
}

impl CaretView for OverflowPanel {
    fn read_text<R>(&self, cx: &App, read: impl FnOnce(&str, &Caret) -> R) -> R {
        read(
            self.editor.read(cx).document().overflow().text(),
            &self.caret,
        )
    }

    fn select(&mut self, caret: Caret, cx: &mut Context<Self>) {
        self.caret = if vim_on(cx) {
            let text = self.text(cx);
            self.vim.settle(&text, caret)
        } else {
            caret
        };
        self.editor.update(cx, |editor, _| editor.close_history());
        self.blink.restart(cx);
        self.autoscroll = true;
        cx.notify();
    }

    fn unmark(&mut self, _: &mut Context<Self>) {
        self.caret = self.caret.unmarked();
    }

    fn edit(&mut self, edit: &CaretEdit, cx: &mut Context<Self>) {
        let done = self
            .editor
            .update(cx, |editor, cx| editor.edit_overflow(edit, cx));
        if !done {
            return;
        }
        self.caret = edit.caret.clone();
        self.blink.restart(cx);
        self.autoscroll = true;
        cx.notify();
    }

    fn painted(&self) -> Option<&PaintedText> {
        self.painted.as_ref()
    }

    fn column(&mut self) -> &mut Option<Column> {
        &mut self.column
    }

    fn takes_text(&self, cx: &App) -> bool {
        matches!(self.vim_mode(cx), None | Some(VimMode::Insert))
    }
}

caret_input_handler!(OverflowPanel);

impl Focusable for OverflowPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for OverflowPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let panel = cx.entity();
        if !vim_on(cx) {
            self.vim = Vim::default();
        }
        let key_context = match self.vim_mode(cx) {
            None => "OverflowPanel",
            Some(VimMode::Normal) => "OverflowPanel VimNormal",
            Some(VimMode::Insert) => "OverflowPanel VimInsert",
            Some(VimMode::Visual | VimMode::VisualLine) => "OverflowPanel VimVisual",
        };
        let element = div()
            .id("overflow-panel")
            .key_context(key_context)
            .track_focus(&self.focus_handle);
        caret_actions(element, cx)
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
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::use_block))
            .on_action(cx.listener(Self::leave))
            .on_key_down(cx.listener(vim_key_down))
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
                    .font_family(theme.text_font.clone())
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
                    .debug_selector(|| "overflow-hint".to_owned())
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
        elevated_surface(theme)
            .max_w(theme.panel_width)
            .px(px(8.))
            .py(px(4.))
            .font_family(theme.text_font.clone())
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
        let shown = panel.read_text(cx, |text, caret| caret.clamped(text));
        let selection = painted.selection_quads(&shown.selection(), theme.selection);
        let caret = shown
            .selection()
            .is_empty()
            .then(|| caret::caret_quad(&painted, shown.offset(), theme.accent))
            .flatten();
        if panel.autoscroll {
            caret::scroll_to_caret(&panel.scroll_handle, caret.as_ref(), px(0.), cx);
            window.request_animation_frame();
        }
        let text_font = window.text_style().font_family;
        self.panel.update(cx, |panel, _| {
            panel.autoscroll = false;
            panel.text_font = text_font;
        });
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
        let caret = prepaint
            .caret
            .take()
            .filter(|_| focus_handle.is_focused(window))
            .filter(|_| self.panel.read(cx).blink.shown(window, cx));
        let caret_bounds = caret.as_ref().map(|caret| caret.bounds);
        if let Some(caret) = caret {
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
        self.panel.update(cx, |panel, _| {
            panel.painted = Some(painted);
            panel.caret_bounds = caret_bounds;
        });
    }
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
        elevated_surface(theme)
            .px(px(6.))
            .py(px(2.))
            .text_size(theme.font_size_hint)
            .text_color(theme.text_muted)
            .child(self.text.clone())
    }
}

/// Ghost, Revive, Stash and the Slot cycle act on the Document only, so in Overflow they do
/// nothing.
impl VimView for OverflowPanel {
    fn vim(&self) -> &Vim {
        &self.vim
    }

    fn vim_mut(&mut self) -> &mut Vim {
        &mut self.vim
    }

    fn vim_command(&mut self, key: VimKey, cx: &mut Context<Self>) -> VimCommand {
        let text = self.text(cx);
        self.vim.key(key, text.as_str(), &self.caret)
    }

    fn run_vim_command(
        &mut self,
        command: VimCommand,
        was: VimMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = self.vim.mode();
        self.editor
            .update(cx, |editor, _| editor.follow_vim_mode(was, now));
        match run_text_command(self, command, cx) {
            Some(VimCommand::Undo(times)) => self.undo_redo(times, Editor::undo_step, cx),
            Some(VimCommand::Redo(times)) => self.undo_redo(times, Editor::redo_step, cx),
            Some(VimCommand::Run(action)) => window.dispatch_action(gpui_action(action), cx),
            Some(_) | None => {}
        }
    }
}
