//! The AI adapter global, and AI runs: one AI call of a view, with its status, its cancel,
//! and the drop of answers that come too late.

use std::fmt;
use std::sync::Arc;

use gpui::{AppContext as _, Context, Global};
use hamish_core::{AiAdapter, AiError, CancelToken};

/// The AI adapter the app calls on the background executor (ADR 0004). `main` sets the
/// `claude -p` backend; tests set a fake. With none set, AI actions fail with a message.
pub struct Ai(pub Arc<dyn AiAdapter>);

impl Global for Ai {}

/// Why an AI run gave no result: no backend, the AI failed, or core refused the request or
/// the answer with `E`. Views format it when they render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiFailure<E> {
    NoBackend,
    Ai(AiError),
    Refused(E),
}

impl<E: fmt::Display> fmt::Display for AiFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBackend => f.write_str("no AI backend is set"),
            Self::Ai(error) => error.fmt(f),
            Self::Refused(error) => error.fmt(f),
        }
    }
}

/// Where an AI run is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiStatus<E> {
    /// Not started, or cancelled.
    Idle,
    /// The AI works on it.
    Running,
    /// Its answer is in the Document.
    Done,
    /// It gave no result; the reason.
    Failed(AiFailure<E>),
}

impl<E> AiStatus<E> {
    /// The status after an answer was applied, or why not. The AI cancelled is no failure.
    fn after(applied: Result<(), AiFailure<E>>) -> Self {
        match applied {
            Ok(()) => Self::Done,
            Err(AiFailure::Ai(AiError::Cancelled)) => Self::Idle,
            Err(failure) => Self::Failed(failure),
        }
    }
}

/// Finds the run with key `K` in view `T`, for the answer of that run.
pub(crate) type RunOf<T, K, E> = for<'a> fn(&'a mut T, K) -> &'a mut AiRun<K, E>;

/// One AI call at a time for a view, with key `K` (what it runs) and refusal `E`.
/// A new start cancels the call that runs; so do [`AiRun::cancel`] and a drop. The answer of a
/// cancelled or replaced call is dropped.
pub(crate) struct AiRun<K, E> {
    key: Option<K>,
    status: AiStatus<E>,
    cancel: CancelToken,
    /// Counts starts, so an answer knows if its run is still the current one.
    start: u64,
}

impl<K, E> Default for AiRun<K, E> {
    fn default() -> Self {
        Self {
            key: None,
            status: AiStatus::Idle,
            cancel: CancelToken::default(),
            start: 0,
        }
    }
}

impl<K, E> Drop for AiRun<K, E> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl<K: Clone + PartialEq + 'static, E: 'static> AiRun<K, E> {
    pub(crate) fn status(&self) -> &AiStatus<E> {
        &self.status
    }

    /// The key of the last start; `None` before the first.
    pub(crate) fn key(&self) -> Option<&K> {
        self.key.as_ref()
    }

    pub(crate) fn is_running(&self) -> bool {
        matches!(self.status, AiStatus::Running)
    }

    /// Stops the call that runs and drops its answer; the status goes back to idle.
    pub(crate) fn cancel<T: 'static>(&mut self, cx: &mut Context<T>) {
        self.cancel.cancel();
        self.start = self.start.wrapping_add(1);
        self.status = AiStatus::Idle;
        cx.notify();
    }

    /// Asks the AI with `call` on the background executor, then `apply` puts the answer in
    /// view `T`. While `key` runs, a start with the same key does nothing; another key
    /// cancels it. `run_of` finds this run again when the answer comes. A refused `request`,
    /// no AI backend, an AI error, or a refused answer fails the run.
    pub(crate) fn start<T: 'static, Request: Send + 'static, Answer: Send + 'static>(
        &mut self,
        key: K,
        request: Result<Request, E>,
        call: fn(&dyn AiAdapter, &Request, &CancelToken) -> Result<Answer, AiError>,
        run_of: RunOf<T, K, E>,
        apply: impl FnOnce(&mut T, &Request, Answer, &mut Context<T>) -> Result<(), E> + 'static,
        cx: &mut Context<T>,
    ) {
        if self.is_running() && self.key.as_ref() == Some(&key) {
            return;
        }
        self.cancel(cx);
        self.key = Some(key.clone());
        let request = match request {
            Ok(request) => request,
            Err(refused) => {
                self.status = AiStatus::Failed(AiFailure::Refused(refused));
                return;
            }
        };
        let Some(ai) = cx.try_global::<Ai>().map(|ai| Arc::clone(&ai.0)) else {
            self.status = AiStatus::Failed(AiFailure::NoBackend);
            return;
        };
        self.cancel = CancelToken::default();
        self.status = AiStatus::Running;
        let cancel = self.cancel.clone();
        let start = self.start;
        let asking = cx.background_spawn(async move {
            let answer = call(ai.as_ref(), &request, &cancel);
            (request, answer)
        });
        cx.spawn(async move |this, cx| {
            let (request, answer) = asking.await;
            this.update(cx, |view, cx| {
                if run_of(view, key.clone()).start != start {
                    return;
                }
                let applied = answer.map_err(AiFailure::Ai).and_then(|answer| {
                    apply(view, &request, answer, cx).map_err(AiFailure::Refused)
                });
                run_of(view, key).status = AiStatus::after(applied);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use gpui::{AppContext as _, Context, Entity, TestAppContext};
    use hamish_core::{
        AiAdapter, AiError, AiRequest, AiTrigger, CancelToken, Document, LabFinding, LabRequest,
        Level, TrimRequest,
    };

    use super::{Ai, AiFailure, AiRun, AiStatus};

    /// Answers each Variants call with `answer` and keeps its cancel token.
    struct FakeAdapter {
        answer: Result<Vec<String>, AiError>,
        cancels: Arc<Mutex<Vec<CancelToken>>>,
    }

    impl AiAdapter for FakeAdapter {
        fn variants(
            &self,
            _request: &AiRequest,
            cancel: &CancelToken,
        ) -> Result<Vec<String>, AiError> {
            if let Ok(mut cancels) = self.cancels.lock() {
                cancels.push(cancel.clone());
            }
            self.answer.clone()
        }

        fn lab_check(
            &self,
            _request: &LabRequest,
            _cancel: &CancelToken,
        ) -> Result<Vec<LabFinding>, AiError> {
            Ok(Vec::new())
        }

        fn trim(
            &self,
            _request: &TrimRequest,
            _cancel: &CancelToken,
        ) -> Result<Vec<std::ops::Range<usize>>, AiError> {
            Ok(Vec::new())
        }
    }

    /// A view with one AI run; it keeps each answer it applies, or refuses it with `refuse`.
    struct View {
        run: AiRun<u8, &'static str>,
        applied: Vec<(u8, Vec<String>)>,
        refuse: Option<&'static str>,
    }

    fn run_of(view: &mut View, _: u8) -> &mut AiRun<u8, &'static str> {
        &mut view.run
    }

    fn request() -> AiRequest {
        Document::new("Title\nThe customer waits.")
            .ai_request(10..18, Level::Word, AiTrigger::ContextMenu)
            .unwrap()
    }

    fn open(
        cx: &mut TestAppContext,
        answer: Option<Result<Vec<String>, AiError>>,
    ) -> (Entity<View>, Arc<Mutex<Vec<CancelToken>>>) {
        let cancels = Arc::new(Mutex::new(Vec::new()));
        if let Some(answer) = answer {
            let fake = FakeAdapter {
                answer,
                cancels: Arc::clone(&cancels),
            };
            cx.update(|cx| cx.set_global(Ai(Arc::new(fake))));
        }
        let view = cx.new(|_| View {
            run: AiRun::default(),
            applied: Vec::new(),
            refuse: None,
        });
        (view, cancels)
    }

    fn start_with(
        view: &Entity<View>,
        key: u8,
        request: Result<AiRequest, &'static str>,
        cx: &mut TestAppContext,
    ) {
        view.update(cx, |view, cx: &mut Context<View>| {
            view.run.start(
                key,
                request,
                |ai, request, cancel| ai.variants(request, cancel),
                run_of,
                move |view, _, texts, _| {
                    if let Some(why) = view.refuse {
                        return Err(why);
                    }
                    view.applied.push((key, texts));
                    Ok(())
                },
                cx,
            );
        });
    }

    fn start(view: &Entity<View>, key: u8, cx: &mut TestAppContext) {
        start_with(view, key, Ok(request()), cx);
    }

    fn status(view: &Entity<View>, cx: &mut TestAppContext) -> AiStatus<&'static str> {
        view.read_with(cx, |view, _| view.run.status().clone())
    }

    fn applied(view: &Entity<View>, cx: &mut TestAppContext) -> Vec<(u8, Vec<String>)> {
        view.read_with(cx, |view, _| view.applied.clone())
    }

    fn cancelled(cancels: &Arc<Mutex<Vec<CancelToken>>>) -> Vec<bool> {
        cancels.lock().map_or_else(
            |_| Vec::new(),
            |cancels| cancels.iter().map(CancelToken::is_cancelled).collect(),
        )
    }

    fn client() -> Vec<String> {
        vec!["client".to_owned()]
    }

    /// Runs the executor until the AI got a call.
    fn until_called(cancels: &Arc<Mutex<Vec<CancelToken>>>, cx: &mut TestAppContext) {
        while cancelled(cancels).is_empty() && cx.executor().tick() {}
    }

    #[gpui::test]
    fn a_run_is_running_until_its_answer_is_applied(cx: &mut TestAppContext) {
        let (view, _) = open(cx, Some(Ok(client())));

        start(&view, 1, cx);
        assert_eq!(status(&view, cx), AiStatus::Running);
        assert_eq!(applied(&view, cx), vec![]);

        cx.run_until_parked();
        assert_eq!(status(&view, cx), AiStatus::Done);
        assert_eq!(applied(&view, cx), vec![(1, vec!["client".to_owned()])]);
    }

    #[gpui::test]
    fn with_no_ai_backend_a_run_fails_at_once(cx: &mut TestAppContext) {
        let (view, _) = open(cx, None);

        start(&view, 1, cx);

        assert_eq!(status(&view, cx), AiStatus::Failed(AiFailure::NoBackend));
    }

    #[gpui::test]
    fn an_ai_error_fails_the_run_and_applies_nothing(cx: &mut TestAppContext) {
        let declined = AiError::Declined("no".to_owned());
        let (view, _) = open(cx, Some(Err(declined.clone())));

        start(&view, 1, cx);
        cx.run_until_parked();

        assert_eq!(status(&view, cx), AiStatus::Failed(AiFailure::Ai(declined)));
        assert_eq!(applied(&view, cx), vec![]);
    }

    #[gpui::test]
    fn a_refused_request_or_answer_fails_the_run(cx: &mut TestAppContext) {
        let (view, cancels) = open(cx, Some(Ok(client())));

        start_with(&view, 1, Err("nothing to ask"), cx);
        assert_eq!(
            status(&view, cx),
            AiStatus::Failed(AiFailure::Refused("nothing to ask"))
        );
        assert_eq!(cancelled(&cancels), Vec::<bool>::new());

        view.update(cx, |view, _| view.refuse = Some("stale"));
        start(&view, 1, cx);
        cx.run_until_parked();
        assert_eq!(
            status(&view, cx),
            AiStatus::Failed(AiFailure::Refused("stale"))
        );
    }

    #[gpui::test]
    fn the_same_key_does_not_start_again_while_it_runs(cx: &mut TestAppContext) {
        let (view, cancels) = open(cx, Some(Ok(client())));

        start(&view, 1, cx);
        start(&view, 1, cx);
        cx.run_until_parked();

        assert_eq!(cancelled(&cancels), vec![false]);
        assert_eq!(applied(&view, cx), vec![(1, vec!["client".to_owned()])]);
    }

    #[gpui::test]
    fn a_new_key_cancels_the_run_and_drops_its_answer(cx: &mut TestAppContext) {
        let (view, cancels) = open(cx, Some(Ok(client())));

        start(&view, 1, cx);
        until_called(&cancels, cx);
        start(&view, 2, cx);
        cx.run_until_parked();

        assert_eq!(cancelled(&cancels), vec![true, false]);
        assert_eq!(applied(&view, cx), vec![(2, vec!["client".to_owned()])]);
        assert_eq!(
            view.read_with(cx, |view, _| view.run.key().copied()),
            Some(2)
        );
    }

    #[gpui::test]
    fn cancel_stops_the_call_and_drops_its_answer(cx: &mut TestAppContext) {
        let (view, cancels) = open(cx, Some(Ok(client())));

        start(&view, 1, cx);
        until_called(&cancels, cx);
        view.update(cx, |view, cx| view.run.cancel(cx));
        cx.run_until_parked();

        assert_eq!(cancelled(&cancels), vec![true]);
        assert_eq!(status(&view, cx), AiStatus::Idle);
        assert_eq!(applied(&view, cx), vec![]);
    }

    #[gpui::test]
    fn a_dropped_run_cancels_its_call(cx: &mut TestAppContext) {
        let (view, cancels) = open(cx, Some(Ok(client())));

        start(&view, 1, cx);
        until_called(&cancels, cx);
        cx.update(|_| drop(view));
        cx.run_until_parked();

        assert_eq!(cancelled(&cancels), vec![true]);
    }

    #[gpui::test]
    fn a_call_the_ai_cancelled_ends_with_no_failure(cx: &mut TestAppContext) {
        let (view, _) = open(cx, Some(Err(AiError::Cancelled)));

        start(&view, 1, cx);
        cx.run_until_parked();

        assert_eq!(status(&view, cx), AiStatus::Idle);
    }
}
