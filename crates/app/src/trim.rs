//! Trim in the Lab (FR-54 to FR-62): the trim row of the Lab popover and the trim bar.
//! A Trim level runs on the background executor; the cuts fade in the editor until the
//! writer makes them.

use std::sync::Arc;

use gpui::{AnyElement, Context, IntoElement, SharedString, Window, div, prelude::*, px};
use ka_teb_core::{AiError, TrimError, TrimLevel, TrimRequest};

use crate::ai::AiFailure;
use crate::{Ai, Theme, Workspace};

#[expect(
    clippy::disallowed_types,
    reason = "gpui's Action derive builds actions from key binding JSON"
)]
mod actions {
    use gpui::Action;
    use ka_teb_core::TrimLevel;

    gpui::actions!(trim, [MakeTrimCuts, WalkTrim, WalkKeep, WalkCut, CloseTrim]);

    /// Runs a Trim at this level; Original removes the Trim fades (FR-55, FR-62).
    #[derive(Debug, Clone, PartialEq, Action)]
    #[action(namespace = trim, no_json)]
    pub struct RunTrim(pub TrimLevel);
}

pub use actions::{CloseTrim, MakeTrimCuts, RunTrim, WalkCut, WalkKeep, WalkTrim};

/// Width of the trim bar (spec estimate).
const BAR_WIDTH: gpui::Pixels = px(390.);

/// What the trim bar shows (FR-56).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimBar {
    pub level: TrimLevel,
    /// "Slight trim 535 → 480 words · -10%", or the level and why it has no proposal yet.
    pub line: String,
    /// A Walk through is on: the bar asks to keep or cut the current cut.
    pub walking: bool,
}

/// The last Trim request that has no proposal yet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum TrimRun {
    #[default]
    Idle,
    Running(TrimLevel),
    Failed(TrimLevel, AiFailure<TrimError>),
}

/// The FR-54 label of `level`.
fn label(level: TrimLevel) -> &'static str {
    match level {
        TrimLevel::Original => "Original",
        TrimLevel::SlightTrim => "Slight trim",
        TrimLevel::TightenMore => "Tighten more",
        TrimLevel::EvenSharper => "Even sharper",
        TrimLevel::CutInHalf => "Cut in half",
    }
}

impl Workspace {
    /// The Trim level with the accent outline: the one that runs, else the one of the open
    /// proposal, else Original.
    #[must_use]
    pub fn trim_level(&self, cx: &gpui::App) -> TrimLevel {
        if let TrimRun::Running(level) = self.lab.trim {
            return level;
        }
        self.editor
            .read(cx)
            .document()
            .trim_proposal()
            .map_or(TrimLevel::Original, ka_teb_core::TrimProposal::level)
    }

    /// The trim bar; `None` when no Trim runs, failed, or has a proposal open.
    #[must_use]
    pub fn trim_bar(&self, cx: &gpui::App) -> Option<TrimBar> {
        let (level, line) = match &self.lab.trim {
            TrimRun::Running(level) => (*level, format!("{}: trimming…", label(*level))),
            TrimRun::Failed(level, failure) => (*level, format!("{}: {failure}", label(*level))),
            TrimRun::Idle => {
                let proposal = self.editor.read(cx).document().trim_proposal()?;
                let line = format!("{} {proposal}", label(proposal.level()));
                (proposal.level(), line)
            }
        };
        let walking = self
            .editor
            .read(cx)
            .document()
            .trim_proposal()
            .is_some_and(|proposal| proposal.current().is_some());
        Some(TrimBar {
            level,
            line,
            walking,
        })
    }

    /// FR-55: sends the whole active text to the AI on the background executor. A new
    /// level replaces the proposal when its answer comes (FR-61); Original removes the
    /// proposal at once (FR-62). A level that runs already is not started again.
    pub(crate) fn run_trim(&mut self, run: &RunTrim, _: &mut Window, cx: &mut Context<Self>) {
        let level = run.0;
        if self.lab.trim == TrimRun::Running(level) {
            return;
        }
        let request = self.editor.read(cx).document().trim_request(level);
        let Some(request) = request else {
            self.end_trim(cx);
            return;
        };
        let Some(ai) = cx.try_global::<Ai>().map(|ai| Arc::clone(&ai.0)) else {
            self.lab.trim = TrimRun::Failed(level, AiFailure::NoBackend);
            cx.notify();
            return;
        };
        self.lab.trim = TrimRun::Running(level);
        cx.notify();
        let trimming = cx.background_spawn(async move {
            let cuts = ai.trim(&request);
            (request, cuts)
        });
        cx.spawn(async move |this, cx| {
            let (request, cuts) = trimming.await;
            this.update(cx, |workspace, cx| workspace.trimmed(&request, cuts, cx))
        })
        .detach();
    }

    /// Makes the answer the proposal. An answer for a level that is not the running one
    /// any more is dropped; a failure or a result for changed text is kept for the bar.
    fn trimmed(
        &mut self,
        request: &TrimRequest,
        cuts: Result<Vec<std::ops::Range<usize>>, AiError>,
        cx: &mut Context<Self>,
    ) {
        let level = request.level();
        if self.lab.trim != TrimRun::Running(level) {
            return;
        }
        let proposed = cuts.map_err(AiFailure::Ai).and_then(|cuts| {
            self.editor
                .update(cx, |editor, cx| {
                    editor.trim_step(|document| document.apply_trim_result(request, &cuts), cx)
                })
                .map_err(AiFailure::Refused)
        });
        self.lab.trim = match proposed {
            Ok(()) => TrimRun::Idle,
            Err(failure) => TrimRun::Failed(level, failure),
        };
        cx.notify();
    }

    /// Removes the proposal and any run or failure; the text stays (FR-60, FR-62).
    fn end_trim(&mut self, cx: &mut Context<Self>) {
        self.lab.trim = TrimRun::Idle;
        self.trim_step(
            |document| {
                document.close_trim();
                Ok(())
            },
            cx,
        );
    }

    /// Runs a Trim `step` in the editor. A failed step changes nothing, so it is dropped.
    fn trim_step(
        &mut self,
        step: impl FnOnce(&mut ka_teb_core::Document) -> Result<(), TrimError>,
        cx: &mut Context<Self>,
    ) {
        let done = self
            .editor
            .update(cx, |editor, cx| editor.trim_step(step, cx));
        if done.is_ok() {
            cx.notify();
        }
    }

    /// Done: closes the trim bar; the text stays (FR-60).
    pub(crate) fn close_trim(&mut self, _: &CloseTrim, _: &mut Window, cx: &mut Context<Self>) {
        self.end_trim(cx);
    }

    /// FR-58: deletes the cuts still in the proposal as one undo step.
    pub(crate) fn make_trim_cuts(
        &mut self,
        _: &MakeTrimCuts,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.trim_step(ka_teb_core::Document::make_trim_cuts, cx);
    }

    /// FR-59: starts a Walk through at the first cut still in the proposal.
    pub(crate) fn walk_trim(&mut self, _: &WalkTrim, _: &mut Window, cx: &mut Context<Self>) {
        self.trim_step(ka_teb_core::Document::walk_trim, cx);
    }

    /// Walk through: keeps the current cut and goes to the next.
    pub(crate) fn walk_keep(&mut self, _: &WalkKeep, _: &mut Window, cx: &mut Context<Self>) {
        self.trim_step(ka_teb_core::Document::walk_keep, cx);
    }

    /// Walk through: cuts the current cut as one undo step and goes to the next.
    pub(crate) fn walk_cut(&mut self, _: &WalkCut, _: &mut Window, cx: &mut Context<Self>) {
        self.trim_step(ka_teb_core::Document::walk_cut, cx);
    }

    /// FR-54: the trim row of the Lab popover: one outlined button per level, the
    /// selected one in accent.
    pub(crate) fn trim_row(&self, theme: &Theme, cx: &gpui::App) -> AnyElement {
        let selected = self.trim_level(cx);
        let buttons = TrimLevel::ALL
            .into_iter()
            .enumerate()
            .map(|(index, level)| {
                let (border, color) = if level == selected {
                    (theme.accent_outline, theme.accent)
                } else {
                    (theme.panel_border, theme.text)
                };
                let percent: SharedString = match level.target_percent() {
                    0 => "".into(),
                    target => format!("~{target}%").into(),
                };
                div()
                    .id(("trim-level", index))
                    .debug_selector(move || format!("trim-level-{index}"))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .py(px(2.))
                    .border_1()
                    .border_color(border)
                    .rounded(px(3.))
                    .cursor_pointer()
                    .hover(|button| button.bg(theme.elevated_row_hover))
                    .on_click(move |_, window, cx| {
                        window.dispatch_action(Box::new(RunTrim(level)), cx);
                    })
                    .child(div().text_color(color).child(label(level)))
                    .child(div().text_color(theme.text_muted).child(percent))
            });
        div()
            .mt(px(4.))
            .mx(px(8.))
            .pt(px(6.))
            .border_t_1()
            .border_color(theme.panel_border)
            .flex()
            .gap(px(4.))
            .text_center()
            .children(buttons)
            .into_any_element()
    }

    /// FR-56: the trim bar, while a Trim runs, failed, or has a proposal open.
    pub(crate) fn trim_bar_view(&self, theme: &Theme, cx: &gpui::App) -> Option<AnyElement> {
        let bar = self.trim_bar(cx)?;
        let proposed = self.lab.trim == TrimRun::Idle;
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
        let action = |name, caption, action: fn() -> Box<dyn gpui::Action>| {
            button(name, caption).on_click(move |_, window, cx| {
                window.dispatch_action(action(), cx);
            })
        };
        let steps: Vec<_> = match (proposed, bar.walking) {
            (false, _) => Vec::new(),
            (true, false) => vec![
                action("trim-make-cuts", "Make the cuts", || Box::new(MakeTrimCuts)),
                action("trim-walk", "Walk through", || Box::new(WalkTrim)),
            ],
            (true, true) => vec![
                action("trim-walk-keep", "Keep", || Box::new(WalkKeep)),
                action("trim-walk-cut", "Cut", || Box::new(WalkCut)),
            ],
        };
        let hint = match (proposed, bar.walking) {
            (false, _) => None,
            (true, false) => Some("Faded words would go. Click one to keep it."),
            (true, true) => Some("Keep the marked words, or cut them."),
        };
        let name = label(bar.level);
        let detail = bar
            .line
            .get(name.len()..)
            .unwrap_or_default()
            .trim_start_matches(':')
            .trim_start()
            .to_owned();
        Some(
            div()
                .id("trim-bar")
                .occlude()
                .w(BAR_WIDTH)
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .bg(theme.elevated_background)
                .rounded(theme.elevated_radius)
                .shadow_md()
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
                .children(hint.map(|hint| div().text_color(theme.text_muted).child(hint)))
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .text_color(theme.text)
                        .children(steps)
                        .child(action("trim-done", "Done", || Box::new(CloseTrim))),
                )
                .into_any_element(),
        )
    }
}
