use app::{ShortcutGroup, Workspace};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use hamish_core::{Document, LoadedSettings, Settings};

fn open(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<Workspace>, &mut VisualTestContext) {
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
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(Document::new("Title\nBody."), window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).expect("the window shows the element");
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn sheet_open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).shortcuts_open())
}

fn groups(cx: &mut VisualTestContext) -> Vec<ShortcutGroup> {
    cx.read(app::shortcut_groups)
}

fn keys_of(groups: &[ShortcutGroup], group: &str, label: &str) -> Option<String> {
    groups
        .iter()
        .find(|shown| shown.name == group)?
        .rows
        .iter()
        .find(|row| row.label == label)
        .map(|row| row.keys.clone())
}

#[gpui::test]
fn the_shortcuts_icon_opens_the_sheet_and_escape_closes_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open(cx, Settings::default());
    assert!(!sheet_open(&workspace, cx));

    click(cx, "shortcuts-button");

    assert!(sheet_open(&workspace, cx));
    assert!(cx.debug_bounds("shortcuts-sheet").is_some());

    cx.simulate_keystrokes("escape");
    cx.simulate_input("A ");

    assert!(!sheet_open(&workspace, cx));
    let text = cx.read(|cx| {
        workspace
            .read(cx)
            .editor()
            .read(cx)
            .document()
            .text()
            .to_owned()
    });
    assert_eq!(text, "A Title\nBody.");
}

#[gpui::test]
fn the_sheet_lists_the_key_bindings_in_groups(cx: &mut TestAppContext) {
    let (_, cx) = open(cx, Settings::default());

    let groups = groups(cx);

    let names: Vec<&str> = groups.iter().map(|group| group.name).collect();
    assert_eq!(names, ["Editor", "Slots", "Overflow", "Lab"]);
    assert_eq!(
        keys_of(&groups, "Editor", "Save").as_deref(),
        Some("Ctrl+S")
    );
    assert_eq!(
        keys_of(&groups, "Slots", "Alternatives for selection").as_deref(),
        Some("Ctrl+Shift+A")
    );
    assert_eq!(
        keys_of(&groups, "Overflow", "Open or close Overflow").as_deref(),
        Some("Ctrl+Shift+O")
    );
    assert_eq!(
        keys_of(&groups, "Lab", "Close the Lab").as_deref(),
        Some("Esc")
    );
}

#[gpui::test]
fn with_vim_mode_on_the_sheet_lists_the_vim_keys(cx: &mut TestAppContext) {
    let settings = Settings {
        vim_mode: true,
        ..Settings::default()
    };
    let (_, cx) = open(cx, settings);

    let groups = groups(cx);

    assert_eq!(groups.last().map(|group| group.name), Some("Vim mode"));
    assert_eq!(
        keys_of(&groups, "Vim mode", "Back to Normal mode").as_deref(),
        Some("Esc")
    );
}
