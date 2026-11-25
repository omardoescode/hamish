//! The hamish binary: loads the settings, sets the sound device and the AI backend, and
//! opens one window on an empty Document.

use std::sync::Arc;

use app::{Ai, DeviceSounds, Sounds, Workspace};
use gpui::{App, Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use hamish_core::{ClaudeBackend, Document, Settings, settings_path};

#[expect(
    clippy::print_stderr,
    reason = "startup errors have no window to show them in"
)]
fn main() {
    let path = settings_path(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::home_dir().as_deref(),
    );
    let loaded = path.as_deref().map(Settings::load);
    Application::new().run(|cx: &mut App| {
        if let Err(error) = app::init(cx) {
            eprintln!("hamish: cannot load the bundled font: {error:#}");
            cx.quit();
            return;
        }
        if let Some(loaded) = loaded {
            app::use_settings(path, loaded, cx);
        }
        app::load_fonts_in_background(cx);
        cx.set_global(Ai(Arc::new(ClaudeBackend::default())));
        cx.set_global(Sounds(Arc::new(DeviceSounds::start())));
        let bounds = Bounds::centered(None, size(px(1140.0), px(720.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("hamish".into()),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            cx.new(|cx| Workspace::new(Document::default(), window, cx))
        });
        if let Err(error) = opened {
            eprintln!("hamish: cannot open window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
