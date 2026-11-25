//! The Settings global: the settings in use, where they save, and how views read them.

use std::path::PathBuf;

use gpui::{App, Global};
use ka_teb_core::{LoadedSettings, Settings, SettingsError};

use crate::{Notice, Theme};

/// The settings in use and where they save; a global.
#[derive(Default)]
pub(crate) struct SettingsStore {
    pub(crate) settings: Settings,
    /// `None`: changes apply but do not save.
    pub(crate) path: Option<PathBuf>,
    /// Why the file at start did not load; the first Workspace shows it.
    problem: Option<SettingsError>,
}

impl Global for SettingsStore {}

/// Uses `loaded` (read from `path` at start) and saves later changes to `path`.
pub fn use_settings(path: Option<PathBuf>, loaded: LoadedSettings, cx: &mut App) {
    cx.set_global(Theme::new(&loaded.settings));
    cx.set_global(SettingsStore {
        settings: loaded.settings,
        path,
        problem: loaded.problem,
    });
}

/// The on/off setting that `pick` reads. `false` when no settings are set.
pub(crate) fn is_on(cx: &App, pick: impl FnOnce(&Settings) -> bool) -> bool {
    cx.try_global::<SettingsStore>()
        .is_some_and(|store| pick(&store.settings))
}

/// The UI scale in use; 1 when no settings are set.
pub(crate) fn ui_scale(cx: &App) -> f32 {
    cx.try_global::<SettingsStore>()
        .map_or(1., |store| store.settings.ui_scale)
}

/// The notice for a settings file that did not load at start; it shows one time.
pub(crate) fn take_start_notice(cx: &mut App) -> Option<Notice> {
    let problem = cx
        .try_global::<SettingsStore>()?
        .problem
        .as_ref()?
        .to_string();
    cx.global_mut::<SettingsStore>().problem = None;
    Some(Notice::SettingsNotLoaded(problem))
}
