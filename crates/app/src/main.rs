use std::sync::Arc;

use app::{Ai, Workspace};
use gpui::{App, Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use ka_teb_core::{ClaudeBackend, Document};

#[expect(
    clippy::print_stderr,
    reason = "startup errors have no window to show them in"
)]
fn main() {
    Application::new().run(|cx: &mut App| {
        if let Err(error) = app::init(cx) {
            eprintln!("ka-teb: cannot load the body font: {error:#}");
            cx.quit();
            return;
        }
        cx.set_global(Ai(Arc::new(ClaudeBackend::default())));
        let bounds = Bounds::centered(None, size(px(1140.0), px(720.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("ka-teb".into()),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            cx.new(|cx| Workspace::new(Document::default(), window, cx))
        });
        if let Err(error) = opened {
            eprintln!("ka-teb: cannot open window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
