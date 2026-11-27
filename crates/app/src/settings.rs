//! The Settings global: the settings in use, where they save, how saves queue, and how
//! views read them.

use std::path::PathBuf;

use gpui::{App, AppContext, Global};
use hamish_core::{LoadedSettings, Settings, SettingsError, Toggle};

/// What a save does when it fails.
type OnSaveFailed = Box<dyn FnOnce(&SettingsError, &mut App)>;

use crate::{Notice, Theme};

/// The settings in use and where they save; a global.
#[derive(Default)]
pub(crate) struct SettingsStore {
    pub(crate) settings: Settings,
    /// `None`: changes apply but do not save.
    pub(crate) path: Option<PathBuf>,
    /// Why the file at start did not load; the first Workspace shows it.
    problem: Option<SettingsError>,
    saving: bool,
    /// A save asked for while one runs; it starts when that one ends.
    queued: Option<OnSaveFailed>,
}

impl Global for SettingsStore {}

/// Uses `loaded` (read from `path` at start) and saves later changes to `path`.
pub fn use_settings(path: Option<PathBuf>, loaded: LoadedSettings, cx: &mut App) {
    cx.set_global(Theme::new(&loaded.settings));
    cx.set_global(SettingsStore {
        settings: loaded.settings,
        path,
        problem: loaded.problem,
        saving: false,
        queued: None,
    });
}

/// Saves the settings in use on the background executor; `failed` gets the error. A save
/// asked for while one runs waits, and then saves the settings in use at that time.
/// Without a path, nothing saves.
pub(crate) fn save(cx: &mut App, failed: impl FnOnce(&SettingsError, &mut App) + 'static) {
    let store = cx.global_mut::<SettingsStore>();
    let Some(path) = store.path.clone() else {
        return;
    };
    if store.saving {
        store.queued = Some(Box::new(failed));
        return;
    }
    store.saving = true;
    let settings = store.settings.clone();
    let saving = cx.background_spawn(async move { settings.save(&path) });
    cx.spawn(async move |cx| {
        let result = saving.await;
        cx.update(|cx| saved(&result, failed, cx));
    })
    .detach();
}

fn saved(
    result: &Result<(), SettingsError>,
    failed: impl FnOnce(&SettingsError, &mut App),
    cx: &mut App,
) {
    let store = cx.global_mut::<SettingsStore>();
    store.saving = false;
    let queued = store.queued.take();
    if let Err(error) = result {
        failed(error, cx);
    }
    if let Some(next) = queued {
        save(cx, next);
    }
}

/// Applies `change` to the settings in use at once, in each window, and saves them; `failed`
/// gets a save error.
pub(crate) fn change(
    cx: &mut App,
    change: impl FnOnce(&mut Settings),
    failed: impl FnOnce(&SettingsError, &mut App) + 'static,
) {
    let store = cx.global_mut::<SettingsStore>();
    change(&mut store.settings);
    let theme = Theme::new(&store.settings);
    cx.set_global(theme);
    cx.refresh_windows();
    save(cx, failed);
}

/// `toggle` is on. `false` when no settings are set.
pub(crate) fn is_on(cx: &App, toggle: Toggle) -> bool {
    cx.try_global::<SettingsStore>()
        .is_some_and(|store| store.settings.is_on(toggle))
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
