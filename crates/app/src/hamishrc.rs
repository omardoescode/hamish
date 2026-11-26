//! The hamishrc global: the key maps in use and where the file is. The file reads on the
//! background executor at start, on window focus, on `:source` and on "Reload hamishrc".

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, AppContext, Context, Global, InteractiveElement, Window};
use hamish_core::{HAMISHRC_EXAMPLE, Hamishrc, Toggle};

use crate::actions::{EditHamishrc, RecursiveMap, ReloadHamishrc};
use crate::save_open::Notice;
use crate::{Workspace, settings};

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

/// The text of the file at `path`; empty when it does not exist.
fn read(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        read => read,
    }
}

/// Writes [`HAMISHRC_EXAMPLE`] to `path` when no file is there.
fn create_if_missing(path: &Path) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut created) => created.write_all(HAMISHRC_EXAMPLE.as_bytes()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
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
            read(&path).map(|text| {
                let rc = Hamishrc::parse(&text);
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
                self.show(
                    Notice::Hamishrc(vec![format!("hamishrc not read: {error}")]),
                    cx,
                );
                return;
            }
        };
        let vim_mode = settings::is_on(cx, |settings| settings.is_on(Toggle::VimMode));
        let store = cx.global_mut::<HamishrcStore>();
        let changed = store.text.as_ref() != Some(&text);
        let errors: Vec<String> = rc.errors().iter().map(ToString::to_string).collect();
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
            let notice = "hamishrc: no config dir (set HOME or XDG_CONFIG_HOME)".to_owned();
            self.show(Notice::Hamishrc(vec![notice]), cx);
            return;
        };
        let creating = cx.background_spawn(async move { create_if_missing(&path).map(|()| path) });
        cx.spawn(async move |this, cx| {
            let created = creating.await;
            this.update(cx, |workspace, cx| match created {
                Ok(path) => cx.open_with_system(&path),
                Err(error) => {
                    let notice = format!("hamishrc not made: {error}");
                    workspace.show(Notice::Hamishrc(vec![notice]), cx);
                }
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
        let notice = format!(
            "hamishrc: a recursive map stopped after {} maps",
            hamish_core::MAX_MAP_DEPTH
        );
        self.show(Notice::Hamishrc(vec![notice]), cx);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use hamish_core::HAMISHRC_EXAMPLE;

    use super::create_if_missing;

    #[test]
    fn a_missing_hamishrc_is_made_with_the_example_and_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hamish").join("hamishrc");

        create_if_missing(&path).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), HAMISHRC_EXAMPLE);
    }

    #[test]
    fn a_hamishrc_that_exists_stays_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hamishrc");
        fs::write(&path, "nnoremap Y y$\n").unwrap();

        create_if_missing(&path).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "nnoremap Y y$\n");
    }
}
