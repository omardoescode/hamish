use std::sync::Arc;

use gpui::Global;
use ka_teb_core::AiAdapter;

/// The AI adapter the app calls on the background executor (ADR 0004). `main` sets the
/// `claude -p` backend; tests set a fake. With none set, AI actions fail with a message.
pub struct Ai(pub Arc<dyn AiAdapter>);

impl Global for Ai {}
