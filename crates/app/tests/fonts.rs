use app::{ToggleChrome, ToggleOverflow, Workspace};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use ka_teb_core::{Document, LoadedSettings, Settings};

/// Opens a window with `settings` and `text`, and shows the Chrome and Overflow.
fn open<'a>(
    cx: &'a mut TestAppContext,
    settings: Settings,
    text: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    cx.update(|cx| {
        app::use_settings(
            None,
            LoadedSettings {
                settings,
                problem: None,
            },
            cx,
        );
    });
    let text = text.to_owned();
    let (workspace, cx) =
        cx.add_window_view(move |window, cx| Workspace::new(Document::new(&text), window, cx));
    cx.dispatch_action(ToggleChrome);
    cx.dispatch_action(ToggleOverflow);
    cx.run_until_parked();
    (workspace, cx)
}

/// The font family of the Chrome, the Document text, and the Overflow text, as last painted.
fn painted_fonts(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> (String, String, String) {
    cx.read(|cx| {
        let workspace = workspace.read(cx);
        (
            workspace.chrome_font().to_string(),
            workspace.editor().read(cx).text_font().to_string(),
            workspace.overflow_panel().read(cx).text_font().to_string(),
        )
    })
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).expect("the window shows the element");
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_chrome_uses_the_ui_font_and_the_document_text_uses_the_text_font(cx: &mut TestAppContext) {
    let settings = Settings {
        ui_font: "DejaVu Sans".to_owned(),
        text_font: Some("DejaVu Serif".to_owned()),
        ..Settings::default()
    };
    let (workspace, cx) = open(cx, settings, "Title\nBody.");

    assert_eq!(
        painted_fonts(&workspace, cx),
        (
            "DejaVu Sans".to_owned(),
            "DejaVu Serif".to_owned(),
            "DejaVu Serif".to_owned()
        )
    );
}

#[gpui::test]
fn the_default_text_font_follows_the_ui_font_when_it_changes(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Settings::default(), "Title\nBody.");
    let plex = "IBM Plex Mono".to_owned();
    assert_eq!(
        painted_fonts(&workspace, cx),
        (plex.clone(), plex.clone(), plex.clone())
    );

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-ui-font-select");
    click(cx, "settings-ui-font-1");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    let (chrome, text, overflow) = painted_fonts(&workspace, cx);
    assert_ne!(chrome, plex);
    assert_eq!(text, chrome);
    assert_eq!(overflow, chrome);
}

#[gpui::test]
fn bold_text_keeps_its_weight_in_a_text_font_with_no_bold_face(cx: &mut TestAppContext) {
    let settings = Settings {
        text_font: Some("Adwaita Sans".to_owned()),
        ..Settings::default()
    };
    let (workspace, cx) = open(cx, settings, "# Head\nBody **b** end.");

    let bold = cx.read(|cx| workspace.read(cx).editor().read(cx).synthetic_bold());

    assert_eq!(bold, vec![0, 1, 2, 3, 4, 5, 12, 13, 14, 15, 16]);
}
