//! The font list of the Settings selects and the font faces loaded at start. Both run on
//! the background executor, so the UI thread does not scan the system fonts.

use std::sync::Arc;

use gpui::{App, AppContext, Font, FontStyle, FontWeight, Global, SharedString, TextSystem, font};
use hamish_core::Settings;

use crate::settings::SettingsStore;

struct FontList(Arc<[SharedString]>);

impl Global for FontList {}

/// The bundled font first, then the fonts the text system knows, with no hidden (`.`)
/// names. `None` until [`load_fonts_in_background`] ends.
#[must_use]
pub fn font_names(cx: &App) -> Option<Arc<[SharedString]>> {
    cx.try_global::<FontList>().map(|list| Arc::clone(&list.0))
}

/// The font list now: the one loaded in the background, else a list made on this thread.
pub(crate) fn font_names_now(cx: &mut App) -> Arc<[SharedString]> {
    if let Some(names) = font_names(cx) {
        return names;
    }
    let names = list(cx.text_system());
    cx.set_global(FontList(Arc::clone(&names)));
    names
}

/// Loads the faces of the UI font and the Text font in use, then makes the font list, on
/// the background executor.
pub fn load_fonts_in_background(cx: &mut App) {
    let text_system = Arc::clone(cx.text_system());
    let settings = cx.global::<SettingsStore>().settings.clone();
    let loading = cx.background_spawn(async move {
        load_faces(&text_system, &settings);
        list(&text_system)
    });
    cx.spawn(async move |cx| {
        let names = loading.await;
        cx.update(|cx| {
            if !cx.has_global::<FontList>() {
                cx.set_global(FontList(names));
            }
        });
    })
    .detach();
}

/// A face loads from disk on its first use; here that use is off the UI thread.
fn load_faces(text_system: &TextSystem, settings: &Settings) {
    for family in [settings.ui_font.as_str(), settings.text_font_family()] {
        for weight in [FontWeight::NORMAL, FontWeight::BOLD] {
            for style in [FontStyle::Normal, FontStyle::Italic] {
                text_system.resolve_font(&Font {
                    weight,
                    style,
                    ..font(SharedString::from(family.to_owned()))
                });
            }
        }
    }
}

fn list(text_system: &TextSystem) -> Arc<[SharedString]> {
    let bundled = Settings::default().ui_font;
    let system = text_system.all_font_names();
    std::iter::once(bundled.clone())
        .chain(
            system
                .into_iter()
                .filter(|name| !name.starts_with('.') && *name != bundled),
        )
        .map(SharedString::from)
        .collect()
}
