use app::Workspace;
use gpui::{App, Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use ka_teb_core::{Document, Settings, settings_path};

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
            eprintln!("ka-teb: cannot load the body font: {error:#}");
            cx.quit();
            return;
        }
        if let Some(loaded) = loaded {
            app::use_settings(path, loaded, cx);
        }
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
