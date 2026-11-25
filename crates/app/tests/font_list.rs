use app::{Workspace, font_names, load_fonts_in_background};
use gpui::{TestAppContext, VisualTestContext};
use hamish_core::Document;

#[gpui::test]
fn the_font_list_loads_in_the_background_with_the_bundled_font(cx: &mut TestAppContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    assert_eq!(cx.update(|cx| font_names(cx)), None);

    cx.update(load_fonts_in_background);
    cx.run_until_parked();

    let names = cx.update(|cx| font_names(cx)).unwrap_or_default();
    assert!(names.iter().any(|name| name == "IBM Plex Mono"));
    assert!(names.iter().all(|name| !name.starts_with('.')));
}

#[gpui::test]
fn the_settings_modal_opens_with_no_font_list_and_does_not_load_one(cx: &mut TestAppContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let (_, cx): (_, &mut VisualTestContext) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new("Title"), window, cx));
    cx.simulate_keystrokes("ctrl-,");
    cx.run_until_parked();

    assert_eq!(cx.update(|_, cx| font_names(cx)), None);
}
