//! The Lab popover and Lab checks (FR-49 to FR-53): the LAB button, the check list with
//! progress, and the card that accepts or dismisses the Lab mark at the cursor. Checks run
//! on the background executor.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    AnyElement, Context, FocusHandle, IntoElement, MouseButton, SharedString, Window, div,
    prelude::*, px,
};
use ka_teb_core::{AiError, LabCheck, LabError, LabFinding, LabMark, LabMarkId, LabRequest};

use gpui::Focusable;

use crate::actions::{Redo, Undo};
use crate::ai::AiFailure;
use crate::trim::TrimRun;
use crate::{Ai, Editor, Theme, Workspace};

#[expect(
    clippy::disallowed_types,
    reason = "gpui's Action derive builds actions from key binding JSON"
)]
mod actions {
    use gpui::Action;
    use ka_teb_core::LabCheck;

    gpui::actions!(lab, [ToggleLab, CloseLab]);

    /// Runs a Lab check from the open popover.
    #[derive(Debug, Clone, PartialEq, Action)]
    #[action(namespace = lab, no_json)]
    pub struct RunLabCheck(pub LabCheck);
}

pub use actions::{CloseLab, RunLabCheck, ToggleLab};

/// Width of the Lab popover (spec estimate).
const POPOVER_WIDTH: gpui::Pixels = px(270.);
/// Width of the Lab mark card.
const CARD_WIDTH: gpui::Pixels = px(320.);
/// Distance from the window bottom to the LAB button, as the Chrome row.
const BUTTON_BOTTOM: gpui::Pixels = px(24.);
/// Distance from the window bottom to a popover above the LAB button.
const POPOVER_BOTTOM: gpui::Pixels = px(48.);

/// One row of the Lab popover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabRow {
    pub check: LabCheck,
    pub label: &'static str,
    /// What the check does; shown while the guide is open (FR-50).
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
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Run {
    Running,
    Done,
    Failed(AiFailure<LabError>),
}

/// The Lab popover state.
pub(crate) struct Lab {
    open: bool,
    guide: bool,
    runs: HashMap<LabCheck, Run>,
    pub(crate) trim: TrimRun,
    focus_handle: FocusHandle,
}

impl Lab {
    pub(crate) fn new(cx: &mut gpui::App) -> Self {
        Self {
            open: false,
            guide: false,
            runs: HashMap::new(),
            trim: TrimRun::Idle,
            focus_handle: cx.focus_handle(),
        }
    }
}

/// The FR-51 label of `check`.
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

/// What `check` does, for the guide (FR-50).
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
        LabStatus::Idle => "".into(),
        LabStatus::Running => "marking…".into(),
        LabStatus::Marked(0) => "no marks".into(),
        LabStatus::Marked(1) => "1 mark".into(),
        LabStatus::Marked(count) => format!("{count} marks").into(),
        LabStatus::Failed(_) => "failed".into(),
    }
}

impl Workspace {
    /// The Lab popover is open.
    #[must_use]
    pub fn lab_open(&self) -> bool {
        self.lab.open
    }

    /// The popover rows in FR-51 order, with each check's status.
    #[must_use]
    pub fn lab_rows(&self, cx: &gpui::App) -> Vec<LabRow> {
        let marks = self.editor.read(cx).document().lab_marks();
        LabCheck::ALL
            .into_iter()
            .map(|check| {
                let status = match self.lab.runs.get(&check) {
                    None => LabStatus::Idle,
                    Some(Run::Running) => LabStatus::Running,
                    Some(Run::Failed(failure)) => LabStatus::Failed(failure.to_string()),
                    Some(Run::Done) => {
                        LabStatus::Marked(marks.iter().filter(|mark| mark.check == check).count())
                    }
                };
                LabRow {
                    check,
                    label: label(check),
                    guide: self.lab.guide.then(|| guide(check)),
                    status,
                }
            })
            .collect()
    }

    /// The Lab mark at the cursor: the first open mark the cursor lies in or on an edge of.
    /// `None` when text is selected.
    #[must_use]
    pub fn lab_mark_at_cursor(&self, cx: &gpui::App) -> Option<LabMark> {
        let editor = self.editor.read(cx);
        let selection = editor.selection();
        if !selection.is_empty() {
            return None;
        }
        editor
            .document()
            .lab_marks()
            .iter()
            .find(|mark| mark.range.start <= selection.start && selection.start <= mark.range.end)
            .cloned()
    }

    /// FR-49: the LAB button opens the popover and gives it focus; a second press closes it.
    pub(crate) fn toggle_lab(
        &mut self,
        _: &ToggleLab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lab.open {
            self.close_lab(&CloseLab, window, cx);
            return;
        }
        self.lab.open = true;
        window.focus(&self.lab.focus_handle);
        cx.notify();
    }

    /// Esc in the popover: closes it and gives focus back to the editor.
    pub(crate) fn close_lab(&mut self, _: &CloseLab, window: &mut Window, cx: &mut Context<Self>) {
        self.lab.open = false;
        window.focus(&self.editor.read(cx).focus_handle(cx));
        cx.notify();
    }

    fn lab_undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, Editor::undo_step);
    }

    fn lab_redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, Editor::redo_step);
    }

    fn toggle_guide(&mut self, cx: &mut Context<Self>) {
        self.lab.guide = !self.lab.guide;
        cx.notify();
    }

    /// FR-52: sends the whole active text to the AI on the background executor. A check
    /// that runs already is not started again.
    fn run_lab_check(&mut self, run: &RunLabCheck, _: &mut Window, cx: &mut Context<Self>) {
        let check = run.0;
        if self.lab.runs.get(&check) == Some(&Run::Running) {
            return;
        }
        let Some(ai) = cx.try_global::<Ai>().map(|ai| Arc::clone(&ai.0)) else {
            self.lab
                .runs
                .insert(check, Run::Failed(AiFailure::NoBackend));
            cx.notify();
            return;
        };
        let request = self.editor.read(cx).document().lab_request(check);
        self.lab.runs.insert(check, Run::Running);
        cx.notify();
        let checking = cx.background_spawn(async move {
            let findings = ai.lab_check(&request);
            (request, findings)
        });
        cx.spawn(async move |this, cx| {
            let (request, findings) = checking.await;
            this.update(cx, |workspace, cx| {
                workspace.lab_checked(&request, findings, cx);
            })
        })
        .detach();
    }

    /// Marks the findings as one undo step, or keeps why the check failed. A result for
    /// text that changed meanwhile is refused.
    fn lab_checked(
        &mut self,
        request: &LabRequest,
        findings: Result<Vec<LabFinding>, AiError>,
        cx: &mut Context<Self>,
    ) {
        let marked = findings.map_err(AiFailure::Ai).and_then(|findings| {
            self.editor
                .update(cx, |editor, cx| {
                    editor.lab_step(|document| document.apply_lab_result(request, &findings), cx)
                })
                .map_err(AiFailure::Refused)
        });
        let run = match marked {
            Ok(_) => Run::Done,
            Err(failure) => Run::Failed(failure),
        };
        self.lab.runs.insert(request.check(), run);
        cx.notify();
    }

    /// FR-53: puts the proposed fix of mark `id` in place, as one undo step.
    fn accept_lab_fix(&mut self, id: LabMarkId, cx: &mut Context<Self>) {
        let accepted = self.editor.update(cx, |editor, cx| {
            editor.lab_step(|document| document.accept_lab_fix(id), cx)
        });
        if accepted.is_ok() {
            cx.notify();
        }
    }

    /// Ends mark `id`; the text stays.
    fn dismiss_lab_mark(&mut self, id: LabMarkId, cx: &mut Context<Self>) {
        let dismissed = self.editor.update(cx, |editor, cx| {
            editor.lab_step(|document| document.dismiss_lab_mark(id), cx)
        });
        if dismissed.is_ok() {
            cx.notify();
        }
    }

    /// The LAB button (with the Chrome), and above it the trim bar over the open popover or
    /// over the card of the mark at the cursor (while the popover is closed).
    pub(crate) fn lab_view(&self, theme: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        if self.chrome_visible() {
            elements.push(self.lab_button(theme, cx));
        }
        let below = if self.lab.open {
            Some(self.lab_popover(theme, cx))
        } else {
            self.lab_mark_at_cursor(cx)
                .map(|mark| self.lab_mark_card(&mark, theme, cx))
        };
        let bar = self.trim_bar_view(theme, cx);
        if bar.is_some() || below.is_some() {
            elements.push(
                bottom_center(POPOVER_BOTTOM)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(6.))
                            .children(bar)
                            .children(below),
                    )
                    .into_any_element(),
            );
        }
        elements
    }

    fn lab_button(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let running = self.lab.runs.values().any(|run| *run == Run::Running)
            || matches!(self.lab.trim, TrimRun::Running(_));
        let color = if self.lab.open {
            theme.accent
        } else {
            theme.text_muted
        };
        bottom_center(BUTTON_BOTTOM)
            .child(
                div()
                    .id("lab-button")
                    .debug_selector(|| "lab-button".to_owned())
                    .px(px(6.))
                    .border_1()
                    .border_dashed()
                    .border_color(color)
                    .rounded(px(3.))
                    .text_size(theme.font_size_hint)
                    .text_color(color)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(|workspace, _, window, cx| {
                        workspace.toggle_lab(&ToggleLab, window, cx);
                    }))
                    .child(if running { "LAB …" } else { "LAB" }),
            )
            .into_any_element()
    }

    fn lab_popover(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let header = div()
            .id("lab-guide")
            .debug_selector(|| "lab-guide".to_owned())
            .flex()
            .justify_between()
            .px(px(12.))
            .py(px(6.))
            .bg(theme.elevated_row_hover.opacity(0.5))
            .cursor_pointer()
            .on_click(cx.listener(|workspace, _, _, cx| workspace.toggle_guide(cx)))
            .child(div().text_color(theme.text).child("The Lab guide…"))
            .child(
                div()
                    .text_color(theme.text_muted)
                    .child("what each idea does"),
            );
        let rows = self
            .lab_rows(cx)
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let check = row.check;
                let detail = match &row.status {
                    LabStatus::Failed(message) => Some(message.clone()),
                    LabStatus::Idle | LabStatus::Running | LabStatus::Marked(_) => {
                        row.guide.map(str::to_owned)
                    }
                };
                div()
                    .id(("lab-check", index))
                    .debug_selector(move || format!("lab-check-{index}"))
                    .px(px(12.))
                    .py(px(2.))
                    .rounded(theme.elevated_radius)
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.elevated_row_hover))
                    .on_click(move |_, window, cx| {
                        window.dispatch_action(Box::new(RunLabCheck(check)), cx);
                    })
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap(px(12.))
                            .child(div().text_color(theme.text).child(row.label))
                            .child(
                                div()
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
        div()
            .id("lab-popover")
            .key_context("Lab")
            .track_focus(&self.lab.focus_handle)
            .on_action(cx.listener(Self::close_lab))
            .on_action(cx.listener(Self::run_lab_check))
            .on_action(cx.listener(Self::lab_undo))
            .on_action(cx.listener(Self::lab_redo))
            .occlude()
            .w(POPOVER_WIDTH)
            .pb(px(4.))
            .bg(theme.elevated_background)
            .rounded(theme.elevated_radius)
            .shadow_md()
            .text_size(theme.font_size_hint)
            .child(header)
            .children(rows)
            .child(self.trim_row(theme, cx))
            .into_any_element()
    }

    fn lab_mark_card(&self, mark: &LabMark, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
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
            div()
                .id(name)
                .debug_selector(move || name.to_owned())
                .px(px(8.))
                .border_1()
                .border_color(theme.text_muted)
                .rounded(px(3.))
                .cursor_pointer()
                .hover(|button| button.bg(theme.elevated_row_hover))
                .child(caption)
        };
        let accept = mark.fix.as_ref().map(|_| {
            button("lab-mark-accept", "Accept")
                .on_click(cx.listener(move |workspace, _, _, cx| workspace.accept_lab_fix(id, cx)))
        });
        let dismiss = button("lab-mark-dismiss", "Dismiss")
            .on_click(cx.listener(move |workspace, _, _, cx| workspace.dismiss_lab_mark(id, cx)));
        let fix = mark.fix.as_ref().map(|fix| {
            div()
                .text_color(theme.text)
                .child(format!("\"{text}\" → \"{fix}\""))
        });
        let note = mark
            .note
            .as_ref()
            .map(|note| div().text_color(theme.text_muted).child(note.clone()));
        div()
            .id("lab-mark-card")
            .occlude()
            .w(CARD_WIDTH)
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .bg(theme.elevated_background)
            .rounded(theme.elevated_radius)
            .shadow_md()
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

/// A full-width row at `bottom` that centers its child.
fn bottom_center(bottom: gpui::Pixels) -> gpui::Div {
    div()
        .absolute()
        .bottom(bottom)
        .left_0()
        .right_0()
        .flex()
        .justify_center()
}
