//! The Lab: the LAB button, the popover with the Lab checks and their progress, and the card
//! that accepts or dismisses the Lab mark at the caret. Checks run on the background executor.

use std::collections::HashMap;

use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, IntoElement, MouseButton, Render,
    SharedString, Subscription, Window, anchored, deferred, div, point, prelude::*, px,
};
use hamish_core::{LabCheck, LabError, LabMark, LabMarkId, TrimError, TrimLevel};

use crate::actions::{Redo, Undo};
use crate::ai::{AiFailure, AiRun, AiStatus};
use crate::chrome::{clickable_row, popover_surface, text_button};
use crate::{Editor, Theme};

#[expect(
    clippy::disallowed_types,
    reason = "gpui's Action derive builds actions from key binding JSON"
)]
mod actions {
    use gpui::Action;
    use hamish_core::LabCheck;

    gpui::actions!(lab, [ToggleLab, CloseLab]);

    /// Runs a Lab check from the open popover.
    #[derive(Debug, Clone, PartialEq, Action)]
    #[action(namespace = lab, no_json)]
    pub struct RunLabCheck(pub LabCheck);
}

pub use actions::{CloseLab, RunLabCheck, ToggleLab};

/// Width of the Lab popover.
const POPOVER_WIDTH: gpui::Pixels = px(270.);
/// Width of the Lab mark card.
const CARD_WIDTH: gpui::Pixels = px(320.);
/// Space between the selected Lab mark and its card.
const CARD_GAP: gpui::Pixels = px(4.);
/// Space between the top of the Chrome bar and a popover above the LAB button.
const POPOVER_GAP: gpui::Pixels = px(8.);
/// Width of an AI bar.
const BAR_WIDTH: gpui::Pixels = px(390.);

/// One row of the Lab popover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabRow {
    pub check: LabCheck,
    pub label: &'static str,
    /// What the check does; shown while the guide is open.
    pub guide: Option<&'static str>,
    pub status: LabStatus,
}

/// Where a Lab check is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabStatus {
    /// Not run since the app started.
    Idle,
    /// The AI works on it.
    Running,
    /// It ran; this many of its marks are open.
    Marked(usize),
    /// It gave no marks; the reason.
    Failed(AiFailure<LabError>),
}

/// The Lab with its Lab checks and Trim. It changes the Document only through the
/// [`Editor`], so each change is one editor undo step.
pub struct Lab {
    pub(crate) editor: Entity<Editor>,
    open: bool,
    guide: bool,
    checks: HashMap<LabCheck, AiRun<LabCheck, LabError>>,
    /// The Trim run; a new level cancels the one that runs.
    pub(crate) trim: AiRun<TrimLevel, TrimError>,
    focus_handle: FocusHandle,
    _editor_changes: Subscription,
}

fn check_run(lab: &mut Lab, check: LabCheck) -> &mut AiRun<LabCheck, LabError> {
    lab.checks.entry(check).or_default()
}

/// The popover label of `check`.
fn label(check: LabCheck) -> &'static str {
    match check {
        LabCheck::PunctuationTypos => "Fix punctuation and typos",
        LabCheck::WeakestSentences => "Mark the weakest sentences",
        LabCheck::LongSentences => "Mark sentences that run long",
        LabCheck::ConvolutedSentences => "Mark convoluted sentences",
        LabCheck::OffToneWords => "Mark words that don't fit the tone",
        LabCheck::HedgesFiller => "Mark hedges and filler",
    }
}

/// What `check` does, for the guide.
fn guide(check: LabCheck) -> &'static str {
    match check {
        LabCheck::PunctuationTypos => "Proposes small fixes; you accept or dismiss each.",
        LabCheck::WeakestSentences => "Marks the sentences that do the least work.",
        LabCheck::LongSentences => "Marks sentences that are long to read.",
        LabCheck::ConvolutedSentences => "Marks sentences that are hard to follow.",
        LabCheck::OffToneWords => "Marks words out of tone with the rest.",
        LabCheck::HedgesFiller => "Marks hedges and words that add nothing.",
    }
}

fn status_text(status: &LabStatus) -> SharedString {
    match status {
        LabStatus::Idle | LabStatus::Running => "".into(),
        LabStatus::Marked(0) => "no marks".into(),
        LabStatus::Marked(1) => "1 mark".into(),
        LabStatus::Marked(count) => format!("{count} marks").into(),
        LabStatus::Failed(_) => "failed".into(),
    }
}

impl Lab {
    pub(crate) fn new(editor: Entity<Editor>, cx: &mut Context<Self>) -> Self {
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        Self {
            editor,
            open: false,
            guide: false,
            checks: HashMap::new(),
            trim: AiRun::default(),
            focus_handle: cx.focus_handle(),
            _editor_changes: editor_changes,
        }
    }

    /// The Lab popover is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The popover rows in the order of [`LabCheck::ALL`], with each check's status.
    #[must_use]
    pub fn rows(&self, cx: &App) -> Vec<LabRow> {
        let marks = self.editor.read(cx).document().lab_marks();
        LabCheck::ALL
            .into_iter()
            .map(|check| {
                let status = match self.checks.get(&check).map(AiRun::status) {
                    None | Some(AiStatus::Idle) => LabStatus::Idle,
                    Some(AiStatus::Running) => LabStatus::Running,
                    Some(AiStatus::Failed(failure)) => LabStatus::Failed(failure.clone()),
                    Some(AiStatus::Done) => {
                        LabStatus::Marked(marks.iter().filter(|mark| mark.check == check).count())
                    }
                };
                LabRow {
                    check,
                    label: label(check),
                    guide: self.guide.then(|| guide(check)),
                    status,
                }
            })
            .collect()
    }

    /// The Lab mark at the caret (see [`hamish_core::Document::lab_mark_at`]). `None` when
    /// text is selected.
    #[must_use]
    pub fn mark_at_caret(&self, cx: &App) -> Option<LabMark> {
        self.editor.read(cx).selected_lab_mark().cloned()
    }

    /// A Lab check or a Trim runs.
    fn is_running(&self) -> bool {
        self.trim.is_running() || self.checks.values().any(AiRun::is_running)
    }

    /// Opens the popover and gives it focus; a second toggle closes it.
    pub(crate) fn toggle(&mut self, _: &ToggleLab, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(&CloseLab, window, cx);
            return;
        }
        self.open = true;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// Esc in the popover: closes it and gives focus back to the editor.
    fn close(&mut self, _: &CloseLab, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, Editor::undo_step);
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, Editor::redo_step);
    }

    fn toggle_guide(&mut self, cx: &mut Context<Self>) {
        self.guide = !self.guide;
        cx.notify();
    }

    /// Sends the whole active text to the AI. A check that runs already is not started
    /// again. The findings become marks as one undo step; a result for changed text is
    /// refused.
    fn run_lab_check(&mut self, run: &RunLabCheck, _: &mut Window, cx: &mut Context<Self>) {
        let check = run.0;
        let request = self.editor.read(cx).document().lab_request(check);
        self.checks.entry(check).or_default().start(
            check,
            Ok(request),
            |ai, request, cancel| ai.lab_check(request, cancel),
            check_run,
            |lab, request, findings, cx| {
                lab.editor
                    .update(cx, |editor, cx| {
                        editor.change_document(
                            |document| document.apply_lab_result(request, &findings),
                            cx,
                        )
                    })
                    .map(|_| ())
            },
            cx,
        );
    }

    /// Puts the proposed fix of mark `id` in place, as one undo step.
    fn accept_fix(&mut self, id: LabMarkId, cx: &mut Context<Self>) {
        let accepted = self.editor.update(cx, |editor, cx| {
            editor.change_document(|document| document.accept_lab_fix(id), cx)
        });
        if accepted.is_ok() {
            cx.notify();
        }
    }

    /// Ends mark `id`; the text stays.
    fn dismiss_mark(&mut self, id: LabMarkId, cx: &mut Context<Self>) {
        let dismissed = self.editor.update(cx, |editor, cx| {
            editor.change_document(|document| document.dismiss_lab_mark(id), cx)
        });
        if dismissed.is_ok() {
            cx.notify();
        }
    }

    fn popover(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let header = div()
            .id("lab-guide")
            .debug_selector(|| "lab-guide".to_owned())
            .flex()
            .justify_between()
            .px(px(12.))
            .py(px(6.))
            .bg(theme.elevated_row_hover.opacity(0.5))
            .cursor_pointer()
            .on_click(cx.listener(|lab, _, _, cx| lab.toggle_guide(cx)))
            .child(div().text_color(theme.text).child("The Lab guide…"))
            .child(
                div()
                    .text_color(theme.text_muted)
                    .child("what each idea does"),
            );
        let rows = self.rows(cx).into_iter().enumerate().map(|(index, row)| {
            let check = row.check;
            let (border, color) = if row.status == LabStatus::Running {
                (theme.accent_outline, theme.accent)
            } else {
                (gpui::transparent_black(), theme.text)
            };
            let detail = match &row.status {
                LabStatus::Failed(failure) => Some(failure.to_string()),
                LabStatus::Idle | LabStatus::Running | LabStatus::Marked(_) => {
                    row.guide.map(str::to_owned)
                }
            };
            clickable_row(theme)
                .id(("lab-check", index))
                .debug_selector(move || format!("lab-check-{index}"))
                .px(px(12.))
                .py(px(2.))
                .border_1()
                .border_color(border)
                .on_click(move |_, window, cx| {
                    window.dispatch_action(Box::new(RunLabCheck(check)), cx);
                })
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(px(12.))
                        .child(
                            div()
                                .debug_selector(move || format!("lab-check-label-{index}"))
                                .flex_1()
                                .min_w_0()
                                .text_color(color)
                                .child(row.label),
                        )
                        .child(
                            div()
                                .debug_selector(move || format!("lab-check-status-{index}"))
                                .flex_none()
                                .whitespace_nowrap()
                                .text_color(theme.text_muted)
                                .child(status_text(&row.status)),
                        ),
                )
                .children(detail.map(|detail| {
                    div()
                        .text_size(theme.font_size_hint)
                        .text_color(theme.text_muted)
                        .child(detail)
                }))
        });
        popover_surface(theme)
            .id("lab-popover")
            .key_context("Lab")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::run_lab_check))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .debug_selector(|| "lab-popover".to_owned())
            .occlude()
            .w(POPOVER_WIDTH)
            .pb(px(4.))
            .border_1()
            .border_color(theme.panel_border)
            .text_size(theme.font_size_hint)
            .child(header)
            .children(rows)
            .child(self.trim_row(theme, cx))
            .into_any_element()
    }

    fn mark_card(&self, mark: &LabMark, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = mark.id;
        let text = self
            .editor
            .read(cx)
            .document()
            .text()
            .get(mark.range.clone())
            .unwrap_or_default()
            .to_owned();
        let button = |name: &'static str, caption: &'static str| {
            text_button(theme, theme.panel_border)
                .id(name)
                .debug_selector(move || name.to_owned())
                .px(px(8.))
                .child(caption)
        };
        let accept = mark.fix.as_ref().map(|_| {
            button("lab-mark-accept", "Accept")
                .on_click(cx.listener(move |lab, _, _, cx| lab.accept_fix(id, cx)))
        });
        let dismiss = button("lab-mark-dismiss", "Dismiss")
            .on_click(cx.listener(move |lab, _, _, cx| lab.dismiss_mark(id, cx)));
        let fix = mark.fix.as_ref().map(|fix| {
            div()
                .text_color(theme.text)
                .child(format!("\"{text}\" → \"{fix}\""))
        });
        let note = mark
            .note
            .as_ref()
            .map(|note| div().text_color(theme.text_muted).child(note.clone()));
        popover_surface(theme)
            .id("lab-mark-card")
            .debug_selector(|| "lab-mark-card".to_owned())
            .occlude()
            .w(CARD_WIDTH)
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .text_size(theme.font_size_hint)
            .child(div().text_color(theme.accent).child(label(mark.check)))
            .children(fix)
            .children(note)
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .text_color(theme.text)
                    .children(accept)
                    .child(dismiss),
            )
            .into_any_element()
    }
}

impl Focusable for Lab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Lab {
    /// The trim bar over the open popover, and the card of the selected mark just under it.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let below = self.open.then(|| self.popover(&theme, cx));
        let card = self.mark_at_caret(cx).and_then(|mark| {
            let under = self
                .editor
                .read(cx)
                .selected_lab_mark_bounds()?
                .bottom_left();
            Some(deferred(
                anchored()
                    .position(under + point(px(0.), CARD_GAP))
                    .snap_to_window()
                    .child(self.mark_card(&mark, &theme, cx)),
            ))
        });
        let bar = self.trim_bar_view(&theme, cx);
        let check_bars: Vec<_> = LabCheck::ALL
            .into_iter()
            .filter(|check| self.checks.get(check).is_some_and(AiRun::is_running))
            .map(|check| ai_bar(&theme, label(check), "marking…".into()).into_any_element())
            .collect();
        top_center(crate::workspace::chrome_bar_height(cx) + POPOVER_GAP)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .children(bar)
                    .children(check_bars)
                    .children(below),
            )
            .children(card)
    }
}

/// The LAB button of the Chrome: it toggles the popover of `lab`.
pub(crate) fn lab_button(lab: &Entity<Lab>, theme: &Theme, cx: &App) -> AnyElement {
    let (open, running) = {
        let lab = lab.read(cx);
        (lab.open, lab.is_running())
    };
    let color = if open { theme.accent } else { theme.text_muted };
    let lab = lab.clone();
    div()
        .id("lab-button")
        .debug_selector(|| "lab-button".to_owned())
        .px(px(6.))
        .border_1()
        .border_dashed()
        .border_color(color)
        .rounded(theme.elevated_radius)
        .text_size(theme.font_size_hint)
        .text_color(color)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(move |_, window, cx| {
            lab.update(cx, |lab, cx| lab.toggle(&ToggleLab, window, cx));
        })
        .child(if running { "LAB …" } else { "LAB" })
        .into_any_element()
}

/// An AI bar above the Lab popover: `name` in bold, then `detail` in accent. A Trim and
/// each running Lab check show one.
pub(crate) fn ai_bar(theme: &Theme, name: &'static str, detail: SharedString) -> gpui::Div {
    popover_surface(theme)
        .debug_selector(|| "ai-bar".to_owned())
        .occlude()
        .w(BAR_WIDTH)
        .p(px(8.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .text_size(theme.font_size_hint)
        .child(
            div()
                .flex()
                .gap(px(8.))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_title)
                        .child(name),
                )
                .child(div().text_color(theme.accent).child(detail)),
        )
}

/// A full-width row at `top` that centers its child.
fn top_center(top: gpui::Pixels) -> gpui::Div {
    div()
        .absolute()
        .top(top)
        .left_0()
        .right_0()
        .flex()
        .justify_center()
}
