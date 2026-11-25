use app::Workspace;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point, px};
use ka_teb_core::Document;

fn open(
    cx: &mut TestAppContext,
    document: Document,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn overflow_open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).overflow_open())
}

#[gpui::test]
fn ctrl_shift_o_opens_and_closes_overflow(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nBody."));
    assert!(!overflow_open(&workspace, cx));

    cx.simulate_keystrokes("ctrl-shift-o");
    assert!(overflow_open(&workspace, cx));

    cx.simulate_keystrokes("ctrl-shift-o");
    assert!(!overflow_open(&workspace, cx));
}

#[gpui::test]
fn the_overflow_button_in_the_chrome_opens_and_closes_overflow(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Document::new("Title\nBody."));
    cx.simulate_click(point(px(30.), px(28.)), Modifiers::none());

    let button = cx
        .debug_bounds("overflow-button")
        .expect("the chrome shows the Overflow button");
    cx.simulate_click(button.center(), Modifiers::none());
    assert!(overflow_open(&workspace, cx));

    cx.simulate_click(button.center(), Modifiers::none());
    assert!(!overflow_open(&workspace, cx));
}
