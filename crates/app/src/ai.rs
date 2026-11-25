//! The AI adapter global, and why an AI run gave no result.

use std::fmt;
use std::sync::Arc;

use gpui::Global;
use ka_teb_core::{AiAdapter, AiError};

/// The AI adapter the app calls on the background executor (ADR 0004). `main` sets the
/// `claude -p` backend; tests set a fake. With none set, AI actions fail with a message.
pub struct Ai(pub Arc<dyn AiAdapter>);

impl Global for Ai {}

/// Why an AI run gave no result: no backend, the AI failed, or core refused the answer
/// with `E`. Views format it when they render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AiFailure<E> {
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
