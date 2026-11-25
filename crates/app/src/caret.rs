//! The GPUI side of the core `Caret`, for the Editor and Overflow: key actions, clipboard,
//! input method, and the painted caret with its scroll.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, Hsla, InteractiveElement, PaintQuad, Pixels, Point,
    ScrollHandle, UTF16Selection, fill, point, px, size,
};
use hamish_core::{Caret, CaretEdit, Cycle, Motion, byte_range, utf16_range};

use crate::Theme;
use crate::actions::{
    Backspace, Copy, Cut, Delete, MoveDown, MoveLeft, MoveRight, MoveToLineEnd, MoveToLineStart,
    MoveUp, MoveWordLeft, MoveWordRight, Newline, Paste, SelectAll, SelectDown, SelectLeft,
    SelectRight, SelectToLineEnd, SelectToLineStart, SelectUp, SelectWordLeft, SelectWordRight,
};
use crate::editor::PaintedText;

const CARET_WIDTH: Pixels = px(2.);

/// The x a run of Up and Down keeps, and the offset where the last of them put the caret.
/// A caret at another offset starts a new run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Column {
    x: Pixels,
    at: usize,
}

/// A view that edits one text with a [`Caret`].
pub(crate) trait CaretView: Sized + 'static {
    /// The value of `read` over the view's text and its caret.
    fn read_text<R>(&self, cx: &App, read: impl FnOnce(&str, &Caret) -> R) -> R;

    /// Shows `caret` and scrolls to it. The next edit starts a new undo step.
    fn select(&mut self, caret: Caret, cx: &mut Context<Self>);

    /// Ends the composed text of the input method; the undo step stays open.
    fn unmark(&mut self, cx: &mut Context<Self>);

    /// Makes `edit` and scrolls to its caret; a refused edit changes nothing.
    fn edit(&mut self, edit: &CaretEdit, cx: &mut Context<Self>);

    /// The text as last painted; `None` before the first paint.
    fn painted(&self) -> Option<&PaintedText>;

    /// The column of the last Up or Down; `None` before the first.
    fn column(&mut self) -> &mut Option<Column>;

    /// `position` (window) where the painted text has it.
    fn to_painted(&self, position: Point<Pixels>) -> Point<Pixels> {
        position
    }

    /// `bounds` in the painted text where the window shows them.
    fn to_window(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        bounds
    }

    /// The text input may change the text now. `false` keeps the text as it is, for
    /// example in Vim Normal mode.
    fn takes_text(&self, _cx: &App) -> bool {
        true
    }

    /// Up or Down with no selection key. `true` when the view used the key for something
    /// else, so the caret stays.
    fn take_vertical_key(&mut self, _cycle: Cycle, _cx: &mut Context<Self>) -> bool {
        false
    }
}

/// `element` with the caret key actions of `V`.
pub(crate) fn caret_actions<V: CaretView, E: InteractiveElement>(
    element: E,
    cx: &mut Context<V>,
) -> E {
    element
        .on_action(cx.listener(|view: &mut V, _: &MoveLeft, _, cx| {
            go(view, Motion::Left, false, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &MoveRight, _, cx| {
            go(view, Motion::Right, false, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &MoveUp, _, cx| {
            if !view.take_vertical_key(Cycle::Previous, cx) {
                vertical(view, -1., false, cx);
            }
        }))
        .on_action(cx.listener(|view: &mut V, _: &MoveDown, _, cx| {
            if !view.take_vertical_key(Cycle::Next, cx) {
                vertical(view, 1., false, cx);
            }
        }))
        .on_action(cx.listener(|view: &mut V, _: &MoveWordLeft, _, cx| {
            go(view, Motion::WordLeft, false, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &MoveWordRight, _, cx| {
            go(view, Motion::WordRight, false, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &MoveToLineStart, _, cx| {
            go(view, Motion::LineStart, false, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &MoveToLineEnd, _, cx| {
            go(view, Motion::LineEnd, false, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectLeft, _, cx| {
            go(view, Motion::Left, true, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectRight, _, cx| {
            go(view, Motion::Right, true, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectWordLeft, _, cx| {
            go(view, Motion::WordLeft, true, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectWordRight, _, cx| {
            go(view, Motion::WordRight, true, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectUp, _, cx| {
            vertical(view, -1., true, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectDown, _, cx| {
            vertical(view, 1., true, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectToLineStart, _, cx| {
            go(view, Motion::LineStart, true, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectToLineEnd, _, cx| {
            go(view, Motion::LineEnd, true, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &SelectAll, _, cx| {
            let all = view.read_text(cx, |text, _| Caret::all(text));
            view.select(all, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &Backspace, _, cx| {
            let edit = view.read_text(cx, |text, caret| caret.delete(text, Motion::Left));
            view.edit(&edit, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &Delete, _, cx| {
            let edit = view.read_text(cx, |text, caret| caret.delete(text, Motion::Right));
            view.edit(&edit, cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &Newline, _, cx| {
            insert(view, "\n", cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &Copy, _, cx| copy(view, cx)))
        .on_action(cx.listener(|view: &mut V, _: &Cut, _, cx| {
            copy(view, cx);
            insert(view, "", cx);
        }))
        .on_action(cx.listener(|view: &mut V, _: &Paste, _, cx| {
            if let Some(pasted) = cx.read_from_clipboard().and_then(|item| item.text()) {
                insert(view, &pasted, cx);
            }
        }))
}

/// A press at window `position`: the caret goes there, or with `extend` the selection
/// grows to there.
pub(crate) fn press<V: CaretView>(
    view: &mut V,
    position: Point<Pixels>,
    extend: bool,
    cx: &mut Context<V>,
) {
    let motion = Motion::To(offset_at(view, position, cx));
    go(view, motion, extend, cx);
}

/// A double press at window `position` selects the word there.
pub(crate) fn select_word<V: CaretView>(
    view: &mut V,
    position: Point<Pixels>,
    cx: &mut Context<V>,
) {
    let offset = offset_at(view, position, cx);
    let word = view.read_text(cx, |text, _| Caret::word(text, offset));
    view.select(word, cx);
}

/// The text offset at window `position`; the caret offset before the first paint.
pub(crate) fn offset_at<V: CaretView>(view: &V, position: Point<Pixels>, cx: &App) -> usize {
    view.read_text(cx, |text, caret| {
        view.painted().map_or(caret.offset(), |painted| {
            painted.offset_for_local(
                view.to_painted(position) - painted.bounds.origin,
                text.len(),
            )
        })
    })
}

fn go<V: CaretView>(view: &mut V, motion: Motion, extend: bool, cx: &mut Context<V>) {
    let caret = view.read_text(cx, |text, caret| {
        if extend {
            caret.extended(text, motion)
        } else {
            caret.moved(text, motion)
        }
    });
    view.select(caret, cx);
}

fn insert<V: CaretView>(view: &mut V, new: &str, cx: &mut Context<V>) {
    let edit = view.read_text(cx, |text, caret| caret.insert(text, new));
    view.edit(&edit, cx);
}

fn copy<V: CaretView>(view: &mut V, cx: &mut Context<V>) {
    let selected = view.read_text(cx, |text, caret| {
        text.get(caret.clamped(text).selection()).map(str::to_owned)
    });
    if let Some(selected) = selected.filter(|selected| !selected.is_empty()) {
        cx.write_to_clipboard(ClipboardItem::new_string(selected));
    }
}

/// Moves the caret `rows` painted rows up (negative) or down, at the x of its [`Column`].
/// Nothing moves before the first paint.
fn vertical<V: CaretView>(view: &mut V, rows: f32, extend: bool, cx: &mut Context<V>) {
    let (offset, len) =
        view.read_text(cx, |text, caret| (caret.clamped(text).offset(), text.len()));
    let kept = view
        .column()
        .filter(|column| column.at == offset)
        .map(|column| column.x);
    let Some((x, to)) = view.painted().and_then(|painted| {
        let caret = painted.position_for(offset)?;
        let x = kept.unwrap_or(caret.x);
        let target = point(x, caret.y + painted.line_height * (rows + 0.5));
        Some((x, painted.offset_for_local(target, len)))
    }) else {
        return;
    };
    go(view, Motion::To(to), extend, cx);
    let at = view.read_text(cx, |text, caret| caret.clamped(text).offset());
    *view.column() = Some(Column { x, at });
}

/// The caret quad at `offset` of the painted text.
pub(crate) fn caret_quad(painted: &PaintedText, offset: usize, color: Hsla) -> Option<PaintQuad> {
    let position = painted.position_for(offset)?;
    Some(fill(
        Bounds::new(
            painted.bounds.origin + position,
            size(CARET_WIDTH, painted.line_height),
        ),
        color,
    ))
}

/// Scrolls the least amount that keeps the caret out of the edge fade.
pub(crate) fn scroll_to_caret(scroll_handle: &ScrollHandle, caret: Option<&PaintQuad>, cx: &App) {
    let Some(caret) = caret else {
        return;
    };
    let margin = cx.global::<Theme>().edge_fade_height;
    let viewport = scroll_handle.bounds();
    let mut offset = scroll_handle.offset();
    let above = viewport.top() + margin - caret.bounds.top();
    let below = caret.bounds.bottom() - (viewport.bottom() - margin);
    if above > px(0.) {
        offset.y += above;
    }
    if below > px(0.) {
        offset.y -= below;
    }
    offset.y = offset.y.min(px(0.));
    scroll_handle.set_offset(offset);
}

pub(crate) fn text_for_range<V: CaretView>(
    view: &V,
    range_utf16: &Range<usize>,
    actual_range: &mut Option<Range<usize>>,
    cx: &App,
) -> Option<String> {
    view.read_text(cx, |text, _| {
        let range = byte_range(text, range_utf16);
        actual_range.replace(utf16_range(text, &range));
        text.get(range).map(str::to_owned)
    })
}

pub(crate) fn selected_text_range<V: CaretView>(view: &V, cx: &App) -> UTF16Selection {
    view.read_text(cx, |text, caret| UTF16Selection {
        range: caret.selected_utf16(text),
        reversed: caret.is_reversed(),
    })
}

pub(crate) fn marked_text_range<V: CaretView>(view: &V, cx: &App) -> Option<Range<usize>> {
    view.read_text(cx, |text, caret| caret.marked_utf16(text))
}

pub(crate) fn replace_text_in_range<V: CaretView>(
    view: &mut V,
    range_utf16: Option<Range<usize>>,
    new: &str,
    cx: &mut Context<V>,
) {
    if !view.takes_text(cx) {
        return;
    }
    let edit = view.read_text(cx, |text, caret| caret.input(text, range_utf16, new));
    view.edit(&edit, cx);
}

pub(crate) fn replace_and_mark_text_in_range<V: CaretView>(
    view: &mut V,
    range_utf16: Option<Range<usize>>,
    new: &str,
    selected_utf16: Option<Range<usize>>,
    cx: &mut Context<V>,
) {
    if !view.takes_text(cx) {
        return;
    }
    let edit = view.read_text(cx, |text, caret| {
        caret.compose(text, range_utf16, new, selected_utf16)
    });
    view.edit(&edit, cx);
}

/// Window bounds of a caret at the start of `range_utf16`; `None` before the first paint.
pub(crate) fn bounds_for_range<V: CaretView>(
    view: &V,
    range_utf16: &Range<usize>,
    cx: &App,
) -> Option<Bounds<Pixels>> {
    let painted = view.painted()?;
    let start = view.read_text(cx, |text, _| byte_range(text, range_utf16).start);
    let quad = caret_quad(painted, start, Hsla::default())?;
    Some(view.to_window(quad.bounds))
}

pub(crate) fn character_index_for_point<V: CaretView>(
    view: &V,
    position: Point<Pixels>,
    cx: &App,
) -> usize {
    let offset = offset_at(view, position, cx);
    view.read_text(cx, |text, _| utf16_range(text, &(offset..offset)).start)
}

/// Implements `EntityInputHandler` for a [`CaretView`] through the functions above.
macro_rules! caret_input_handler {
    ($view:ty) => {
        impl gpui::EntityInputHandler for $view {
            fn text_for_range(
                &mut self,
                range_utf16: std::ops::Range<usize>,
                actual_range: &mut Option<std::ops::Range<usize>>,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) -> Option<String> {
                $crate::caret::text_for_range(self, &range_utf16, actual_range, cx)
            }

            fn selected_text_range(
                &mut self,
                _ignore_disabled_input: bool,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) -> Option<gpui::UTF16Selection> {
                Some($crate::caret::selected_text_range(self, cx))
            }

            fn marked_text_range(
                &self,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) -> Option<std::ops::Range<usize>> {
                $crate::caret::marked_text_range(self, cx)
            }

            fn unmark_text(&mut self, _: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
                $crate::caret::CaretView::unmark(self, cx);
            }

            fn replace_text_in_range(
                &mut self,
                range_utf16: Option<std::ops::Range<usize>>,
                text: &str,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) {
                $crate::caret::replace_text_in_range(self, range_utf16, text, cx);
            }

            fn replace_and_mark_text_in_range(
                &mut self,
                range_utf16: Option<std::ops::Range<usize>>,
                text: &str,
                new_selected_range_utf16: Option<std::ops::Range<usize>>,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) {
                $crate::caret::replace_and_mark_text_in_range(
                    self,
                    range_utf16,
                    text,
                    new_selected_range_utf16,
                    cx,
                );
            }

            fn bounds_for_range(
                &mut self,
                range_utf16: std::ops::Range<usize>,
                _element_bounds: gpui::Bounds<gpui::Pixels>,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) -> Option<gpui::Bounds<gpui::Pixels>> {
                $crate::caret::bounds_for_range(self, &range_utf16, cx)
            }

            fn character_index_for_point(
                &mut self,
                point: gpui::Point<gpui::Pixels>,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) -> Option<usize> {
                Some($crate::caret::character_index_for_point(self, point, cx))
            }
        }
    };
}
pub(crate) use caret_input_handler;
