use gpui::{
    App, Application, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    rgb, size,
};

struct Workspace;

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0x00ff_ffff))
    }
}

#[expect(
    clippy::print_stderr,
    reason = "the open-window error has no window to show it in"
)]
fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("ka-teb".into()),
            ..Default::default()
        };
        if let Err(error) = cx.open_window(options, |_, cx| cx.new(|_| Workspace)) {
            eprintln!("ka-teb: cannot open window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
