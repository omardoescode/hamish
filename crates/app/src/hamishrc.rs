//! The hamishrc global: the key maps in use and where the file is. The file reads on the
//! background executor at start, on window focus, on `:source` and on "Reload hamishrc".

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, AppContext, Context, Global, InteractiveElement, Window};
use hamish_core::{Hamishrc, MapMode, Toggle, VimKey, create_hamishrc, hamishrc_text};

use crate::actions::{EditHamishrc, RecursiveMap, ReloadHamishrc};
use crate::save_open::Notice;
use crate::{Workspace, settings};

/// The keys that a GPUI key binding takes before Vim mode reads them (`crate::init`), per
/// map mode. Escape and the Ctrl shortcuts in each mode; Enter, Backspace, Ctrl+X and Ctrl+V
/// only in Insert mode.
fn bound_keys() -> Vec<(MapMode, VimKey)> {
    let each_mode = [VimKey::Escape]
        .into_iter()
        .chain("acz/son,".chars().map(VimKey::Ctrl));
    let insert_only = [
        VimKey::Enter,
        VimKey::Backspace,
        VimKey::Ctrl('x'),
        VimKey::Ctrl('v'),
    ];
    [MapMode::Normal, MapMode::Insert, MapMode::Visual]
        .into_iter()
        .flat_map(|mode| each_mode.clone().map(move |key| (mode, key)))
        .chain(insert_only.map(|key| (MapMode::Insert, key)))
        .collect()
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
        let reading = cx.background_spawn(async move {
            hamishrc_text(&path).map(|text| {
                let rc = Hamishrc::parse(&text, &bound_keys());
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
