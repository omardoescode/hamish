//! The hamishrc global: the key maps in use and where the file is. The file reads on the
//! background executor at start, on window focus, on `:source` and on "Reload hamishrc".

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    App, AppContext, AsKeystroke, Context, Global, InteractiveElement, KeyContext, Keymap,
    Keystroke, Window,
};
use hamish_core::{Hamishrc, MapMode, Toggle, VimKey, create_hamishrc, hamishrc_text};

use crate::actions::{EditHamishrc, RecursiveMap, ReloadHamishrc};
use crate::editor::vim_key;
use crate::save_open::Notice;
use crate::{Workspace, settings};

/// The keys that a key binding of [`crate::key_bindings`] takes before Vim mode reads them,
/// per map mode, in the Editor or in Overflow. A key with Shift is not in the list: the Vim key
/// without Shift still reaches the maps.
fn bound_keys() -> Vec<(MapMode, VimKey)> {
    let keymap = Keymap::new(crate::key_bindings());
    let keystrokes: Vec<Keystroke> = keymap
        .bindings()
        .filter_map(|binding| match binding.keystrokes() {
            [keystroke] => Some(keystroke.as_keystroke().clone()),
            _ => None,
        })
        .filter(|keystroke| !keystroke.modifiers.shift && vim_key(keystroke) != VimKey::Other)
        .collect();
    let modes = [
        (MapMode::Normal, "VimNormal"),
        (MapMode::Insert, "VimInsert"),
        (MapMode::Visual, "VimVisual"),
    ];
    let mut bound = Vec::new();
    for (mode, vim_context) in modes {
        let stacks = ["Editor", "OverflowPanel"].map(|view| {
            let mut context = KeyContext::default();
            context.add(view);
            context.add(vim_context);
            let mut workspace = KeyContext::default();
            workspace.add("Workspace");
            [workspace, context]
        });
        for keystroke in &keystrokes {
            let taken = stacks.iter().any(|stack| {
                let (bindings, _) =
                    keymap.bindings_for_input(std::slice::from_ref(keystroke), stack);
                !bindings.is_empty()
            });
            let key = (mode, vim_key(keystroke));
            if taken && !bound.contains(&key) {
                bound.push(key);
            }
        }
    }
    bound
}

/// The hamishrc in use and its file; a global.
#[derive(Default)]
pub(crate) struct HamishrcStore {
    rc: Arc<Hamishrc>,
    /// `None`: no file, so no maps.
    path: Option<PathBuf>,
    /// The text of the last read; `None` before the first.
    text: Option<String>,
}

impl Global for HamishrcStore {}

/// Reads the hamishrc from `path` from now on: at each Workspace start, on window focus, on
/// `:source` and on "Reload hamishrc".
pub fn use_hamishrc(path: Option<PathBuf>, cx: &mut App) {
    cx.set_global(HamishrcStore {
        path,
        ..HamishrcStore::default()
    });
}

/// The hamishrc in use; no maps when none is set.
pub(crate) fn current(cx: &App) -> Arc<Hamishrc> {
    cx.try_global::<HamishrcStore>()
        .map_or_else(Arc::default, |store| Arc::clone(&store.rc))
}

fn path(cx: &App) -> Option<PathBuf> {
    cx.try_global::<HamishrcStore>()?.path.clone()
}

impl Workspace {
    /// `root` with the hamishrc actions of the Workspace.
    pub(crate) fn hamishrc_actions(root: gpui::Div, cx: &Context<Self>) -> gpui::Div {
        root.on_action(cx.listener(|_, _: &ReloadHamishrc, _, cx| {
            Self::reload_hamishrc(true, cx);
        }))
        .on_action(cx.listener(Self::edit_hamishrc))
        .on_action(cx.listener(Self::recursive_map))
    }

    /// Reads the hamishrc again on the background executor. Its errors show as a Notice
    /// when `asked` (`:source`, "Reload hamishrc"), or when the text changed and Vim mode is on.
    pub(crate) fn reload_hamishrc(asked: bool, cx: &mut Context<Self>) {
        let Some(path) = path(cx) else {
            return;
        };
        let bound = bound_keys();
        let reading = cx.background_spawn(async move {
            hamishrc_text(&path).map(|text| {
                let rc = Hamishrc::parse(&text, &bound);
                (text, rc)
            })
        });
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |workspace, cx| workspace.hamishrc_read(read, asked, cx))
        })
        .detach();
    }

    fn hamishrc_read(
        &mut self,
        read: io::Result<(String, Hamishrc)>,
        asked: bool,
        cx: &mut Context<Self>,
    ) {
        let (text, rc) = match read {
            Ok(read) => read,
            Err(error) => {
                self.show(Notice::HamishrcNotRead(error.to_string()), cx);
                return;
            }
        };
        let vim_mode = settings::is_on(cx, Toggle::VimMode);
        let store = cx.global_mut::<HamishrcStore>();
        let changed = store.text.as_ref() != Some(&text);
        let errors = rc.errors().to_vec();
        store.text = Some(text);
        store.rc = Arc::new(rc);
        if !errors.is_empty() && (asked || (changed && vim_mode)) {
            self.show(Notice::Hamishrc(errors), cx);
        }
    }

    /// "Edit hamishrc": opens the hamishrc in the system default editor. A missing file is
    /// made first, with a commented example. A failure of the editor itself shows no Notice:
    /// GPUI only logs it.
    pub(crate) fn edit_hamishrc(
        &mut self,
        _: &EditHamishrc,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = path(cx) else {
            self.show(Notice::NoHamishrcDir, cx);
            return;
        };
        let creating = cx.background_spawn(async move { create_hamishrc(&path).map(|()| path) });
        cx.spawn(async move |this, cx| {
            let created = creating.await;
            this.update(cx, |workspace, cx| match created {
                Ok(path) => cx.open_with_system(&path),
                Err(error) => workspace.show(Notice::HamishrcNotMade(error.to_string()), cx),
            })
        })
        .detach();
    }

    /// A recursive map went through too many maps; its keys are gone.
    pub(crate) fn recursive_map(
        &mut self,
        _: &RecursiveMap,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show(Notice::RecursiveMap, cx);
    }
}

#[cfg(test)]
mod tests {
    use hamish_core::{MapMode, VimKey};

    use super::bound_keys;

    #[test]
    fn escape_and_ctrl_shortcuts_are_bound_in_each_mode_and_text_edit_keys_only_in_insert_mode() {
        let shortcuts = [
            VimKey::Escape,
            VimKey::Ctrl('a'),
            VimKey::Ctrl('c'),
            VimKey::Ctrl('z'),
            VimKey::Ctrl('/'),
            VimKey::Ctrl('s'),
            VimKey::Ctrl('o'),
            VimKey::Ctrl('n'),
            VimKey::Ctrl(','),
        ];
        let text_edits = [
            VimKey::Enter,
            VimKey::Backspace,
            VimKey::Ctrl('x'),
            VimKey::Ctrl('v'),
        ];
        let bound = bound_keys();

        for mode in [MapMode::Normal, MapMode::Insert, MapMode::Visual] {
            for key in shortcuts {
                assert!(bound.contains(&(mode, key)), "{mode:?} {key:?}");
            }
        }
        for key in text_edits {
            assert!(bound.contains(&(MapMode::Insert, key)), "{key:?}");
            assert!(!bound.contains(&(MapMode::Normal, key)), "{key:?}");
            assert!(!bound.contains(&(MapMode::Visual, key)), "{key:?}");
        }
        assert!(!bound.contains(&(MapMode::Normal, VimKey::Ctrl('g'))));
        assert_eq!(bound.len(), 3 * shortcuts.len() + text_edits.len());
    }
}
