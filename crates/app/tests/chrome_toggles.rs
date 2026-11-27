mod system_text;

use std::path::PathBuf;

use app::Workspace;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use hamish_core::{Document, Level, Provenance, Settings, Toggle};

/// Opens "Title\nIt is a **tension** here." with a Word Slot on "tension", and the settings
/// file `path`. The Text font is Adwaita Sans: it has no bold face, so its bold text shows as
/// synthetic bold glyphs.
#[expect(clippy::expect_used, reason = "test fixture")]
fn open(cx: &mut TestAppContext, path: PathBuf) -> (Entity<Workspace>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let mut loaded = Settings::load(&path);
    loaded.settings.text_font = Some("Adwaita Sans".to_owned());
    cx.update(|cx| app::use_settings(Some(path), loaded, cx));
    let mut document = Document::new("Title\nIt is a **tension** here.");
    document
        .create_slot(16..23, Level::Word, "struggle", Provenance::Writer)
        .expect("valid range");
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::new(document, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).expect("the window shows the element");
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn marker_count(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    cx.read(|cx| workspace.read(cx).editor().read(cx).markers().len())
}

fn underline_points(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    cx.read(|cx| {
        let editor = workspace.read(cx).editor().read(cx);
        editor
            .document()
            .slots()
            .iter()
            .map(|slot| editor.underline(slot.id).len())
            .sum()
    })
}

fn bold_glyphs(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<usize> {
    cx.read(|cx| workspace.read(cx).editor().read(cx).synthetic_bold())
}

#[test]
fn the_variant_dots_icon_hides_and_shows_the_markers_and_saves() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (workspace, cx) = open(cx, path.clone());
        assert_eq!(marker_count(&workspace, cx), 1);
        assert!(underline_points(&workspace, cx) > 0);

        click(cx, "markers-button");

        assert_eq!(marker_count(&workspace, cx), 0);
        assert_eq!(underline_points(&workspace, cx), 0);
        assert!(!Settings::load(&path).settings.is_on(Toggle::Markers));

        click(cx, "markers-button");

        assert_eq!(marker_count(&workspace, cx), 1);
        assert!(Settings::load(&path).settings.is_on(Toggle::Markers));
    });
}

#[test]
fn the_markdown_icon_turns_the_markdown_styles_off_and_on_and_saves() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (workspace, cx) = open(cx, path.clone());
        let title_and_bold = vec![0, 1, 2, 3, 4, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24];
        assert_eq!(bold_glyphs(&workspace, cx), title_and_bold);

        click(cx, "markdown-button");

        assert_eq!(bold_glyphs(&workspace, cx), vec![0, 1, 2, 3, 4]);
        assert!(!Settings::load(&path).settings.is_on(Toggle::MarkdownStyles));

        click(cx, "markdown-button");

        assert_eq!(bold_glyphs(&workspace, cx), title_and_bold);
        assert!(Settings::load(&path).settings.is_on(Toggle::MarkdownStyles));
    });
}

#[test]
fn markers_and_markdown_styles_turn_off_in_settings() {
    system_text::run(1, |cx| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (workspace, cx) = open(cx, path.clone());

        cx.simulate_keystrokes("ctrl-,");
        click(cx, "settings-section-editor");
        click(cx, "settings-markers");
        click(cx, "settings-markdown");

        assert_eq!(marker_count(&workspace, cx), 0);
        assert_eq!(bold_glyphs(&workspace, cx), vec![0, 1, 2, 3, 4]);
        let saved = Settings::load(&path).settings;
        assert!(!saved.is_on(Toggle::Markers) && !saved.is_on(Toggle::MarkdownStyles));
    });
}
