//! The Shortcuts sheet: the key bindings in groups, as the keymap holds them. The Shortcuts
//! icon of the Chrome bar opens it; Esc closes it.

use gpui::{
    Action, App, Context, Div, Focusable, IntoElement, MouseButton, MouseDownEvent, Window, div,
    prelude::*, px,
};
use hamish_core::{key_label, vim_keys};

use crate::actions::{
    ClosePanel, CloseShortcuts, ConfirmVariant, Copy, Cut, DeleteVariant, LeaveOverflow,
    MoveWordLeft, MoveWordRight, NewDocument, Open, OpenShortcuts, Paste, Redo, Save, SaveAs,
    SelectAll, Stash, ToggleOverflow, Undo, UseOverflowBlock,
};
use crate::chrome::elevated_surface;
use crate::lab::CloseLab;
use crate::workspace::Visibility;
use crate::{
    AiAlternatives, DeleteActiveVariant, OpenAlternatives, OpenSettings, Theme, ToggleGhost,
    Workspace, hamishrc, settings,
};

/// One group of the Shortcuts sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutGroup {
    pub name: &'static str,
    pub rows: Vec<ShortcutRow>,
}

/// One row of the Shortcuts sheet: what the keys do, and the keys ("Ctrl+Shift+O").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRow {
    pub label: &'static str,
    pub keys: String,
}

/// The keys of a row: bound to an action in the keymap, or read by a view itself.
enum Keys {
    Bound(Box<dyn Action>),
    Fixed(&'static str),
}

fn bound(action: impl Action) -> Keys {
    Keys::Bound(Box::new(action))
}

/// The groups of the Shortcuts sheet: Editor, Slots, Overflow, Lab, and Vim mode while it is
/// on, with the leader of the hamishrc. A bound row shows the keys of its last binding (the one that wins); a row with no
/// binding does not show.
#[must_use]
pub fn shortcut_groups(cx: &App) -> Vec<ShortcutGroup> {
    let groups = [
        (
            "Editor",
            vec![
                ("New Document", bound(NewDocument)),
                ("Open", bound(Open)),
                ("Save", bound(Save)),
                ("Save as", bound(SaveAs)),
                ("Undo", bound(Undo)),
                ("Redo", bound(Redo)),
                ("Copy", bound(Copy)),
                ("Cut", bound(Cut)),
                ("Paste", bound(Paste)),
                ("Select all", bound(SelectAll)),
                ("Word left", bound(MoveWordLeft)),
                ("Word right", bound(MoveWordRight)),
                ("Settings", bound(OpenSettings)),
            ],
        ),
        (
            "Slots",
            vec![
                ("Alternatives for selection", bound(OpenAlternatives)),
                ("AI alternatives", bound(AiAlternatives)),
                (
                    "Cycle the Active variant (over a Slot)",
                    Keys::Fixed("Up / Down"),
                ),
                ("Delete the Active variant", bound(DeleteActiveVariant)),
                ("Ghost or Revive", bound(ToggleGhost)),
                ("Panel: use the Variant", bound(ConfirmVariant)),
                ("Panel: delete the Variant", bound(DeleteVariant)),
                ("Panel: close", bound(ClosePanel)),
            ],
        ),
        (
            "Overflow",
            vec![
                ("Open or close Overflow", bound(ToggleOverflow)),
                ("Stash the selection", bound(Stash)),
                ("Use the Overflow block", bound(UseOverflowBlock)),
                ("Back to the Document", bound(LeaveOverflow)),
            ],
        ),
        ("Lab", vec![("Close the Lab", bound(CloseLab))]),
    ];
    let os = cx.platform_info().os;
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    let keys_of = |keys: &Keys| match keys {
        Keys::Fixed(keys) => Some((*keys).to_owned()),
        Keys::Bound(action) => keymap
            .bindings_for_action(action.as_ref())
            .last()
            .map(|binding| {
                let strokes: Vec<String> = binding
                    .keystrokes()
                    .iter()
                    .map(|stroke| stroke.unparse(os))
                    .collect();
                key_label(&strokes.join(" "))
            }),
    };
    let mut shown: Vec<ShortcutGroup> = groups
        .into_iter()
        .map(|(name, rows)| ShortcutGroup {
            name,
            rows: rows
                .iter()
                .filter_map(|(label, keys)| keys_of(keys).map(|keys| ShortcutRow { label, keys }))
                .collect(),
        })
        .collect();
    if settings::is_on(cx, |settings| settings.vim_mode) {
        shown.push(ShortcutGroup {
            name: "Vim mode",
            rows: vim_keys(hamishrc::current(cx).leader())
                .into_iter()
                .map(|(label, keys)| ShortcutRow { label, keys })
                .collect(),
        });
    }
    shown
}

impl Workspace {
    /// The Shortcuts sheet is open.
    #[must_use]
    pub fn shortcuts_open(&self) -> bool {
        self.shortcuts_shown.is_shown()
    }

    /// The Shortcuts icon: opens the sheet and gives it the keyboard.
    pub(crate) fn open_shortcuts(
        &mut self,
        _: &OpenShortcuts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.shortcuts_shown = Visibility::Shown;
        window.focus(&self.shortcuts_focus, cx);
        cx.notify();
    }

    /// Esc: closes the sheet and gives the keyboard back to the Document.
    pub(crate) fn close_shortcuts(
        &mut self,
        _: &CloseShortcuts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.shortcuts_shown = Visibility::Hidden;
        window.focus(&self.editor.focus_handle(cx), cx);
        cx.notify();
    }

    /// The sheet over the page: one column per group, each row its label and keys.
    pub(crate) fn shortcuts_sheet(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let scale = theme.font_size_hint / px(10.);
        div()
            .id("shortcuts-backdrop")
            .key_context("ShortcutsSheet")
            .track_focus(&self.shortcuts_focus)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.background.opacity(0.6))
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
                window.dispatch_action(Box::new(CloseShortcuts), cx);
            })
            .child(
                elevated_surface(theme)
                    .id("shortcuts-sheet")
                    .debug_selector(|| "shortcuts-sheet".to_owned())
                    .max_w(px(880.) * scale)
                    .p(px(20.))
                    .flex()
                    .flex_wrap()
                    .gap(px(28.))
                    .border_1()
                    .border_color(theme.panel_border)
                    .text_size(theme.font_size_overflow)
                    .line_height(theme.line_height_overflow)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(
                        shortcut_groups(cx)
                            .into_iter()
                            .map(|group| group_column(group, theme)),
                    ),
            )
    }
}

fn group_column(group: ShortcutGroup, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(div().mb(px(4.)).text_color(theme.accent).child(group.name))
        .children(group.rows.into_iter().map(|row| {
            div()
                .flex()
                .justify_between()
                .gap(px(24.))
                .child(div().text_color(theme.text).child(row.label))
                .child(div().text_color(theme.text_muted).child(row.keys))
        }))
}
