//! The Settings modal (issue #21), Zed style: sections on the left, the fields of the
//! active section on the right. A change applies at once and saves to the settings file
//! on the background executor.

use std::path::PathBuf;

use gpui::{
    App, ClickEvent, Context, Div, EventEmitter, FocusHandle, Focusable, Global, IntoElement,
    MouseDownEvent, Render, SharedString, Stateful, Window, div, prelude::*, px,
};
use ka_teb_core::{LoadedSettings, Setting, Settings, SettingsError, ThemeName};

use crate::actions::{CloseSettings, OpenSettings};
use crate::{Notice, Theme, Workspace};

/// The settings in use and where they save; a global.
#[derive(Default)]
pub(crate) struct SettingsStore {
    settings: Settings,
    /// `None`: changes apply but do not save.
    path: Option<PathBuf>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Appearance,
    Editor,
}

/// A settings save failed.
pub(crate) struct SaveFailed(pub(crate) String);

pub struct SettingsModal {
    focus_handle: FocusHandle,
    section: Section,
    /// The bundled font first, then the fonts the text system knows.
    fonts: Vec<SharedString>,
    saving: bool,
    /// A save asked for while one runs; it starts when that one ends.
    queued: bool,
}

impl EventEmitter<SaveFailed> for SettingsModal {}

impl SettingsModal {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let bundled = Settings::default().font_family;
        let system = cx.text_system().all_font_names();
        let fonts = std::iter::once(bundled.clone())
            .chain(
                system
                    .into_iter()
                    .filter(|name| !name.starts_with('.') && *name != bundled),
            )
            .map(SharedString::from)
            .collect();
        Self {
            focus_handle: cx.focus_handle(),
            section: Section::Appearance,
            fonts,
            saving: false,
            queued: false,
        }
    }

    fn change(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        let store = cx.global_mut::<SettingsStore>();
        change(&mut store.settings);
        let theme = Theme::new(&store.settings);
        cx.set_global(theme);
        cx.refresh_windows();
        self.save(cx);
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let store = cx.global::<SettingsStore>();
        let Some(path) = store.path.clone() else {
            return;
        };
        if self.saving {
            self.queued = true;
            return;
        }
        self.saving = true;
        let settings = store.settings.clone();
        let saving = cx.background_spawn(async move { settings.save(&path) });
        cx.spawn(async move |this, cx| {
            let result = saving.await;
            this.update(cx, |modal, cx| modal.saved(result, cx))
        })
        .detach();
    }

    fn saved(&mut self, result: Result<(), SettingsError>, cx: &mut Context<Self>) {
        self.saving = false;
        if let Err(error) = result {
            cx.emit(SaveFailed(error.to_string()));
        }
        if self.queued {
            self.queued = false;
            self.save(cx);
        }
    }

    fn show(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn section_row(&self, section: Section, theme: &Theme, cx: &Context<Self>) -> Stateful<Div> {
        let (id, label) = match section {
            Section::Appearance => ("settings-section-appearance", "Appearance"),
            Section::Editor => ("settings-section-editor", "Editor"),
        };
        let active = self.section == section;
        div()
            .id(id)
            .debug_selector(|| id.to_owned())
            .px(px(8.))
            .py(px(4.))
            .rounded(theme.elevated_radius)
            .cursor_pointer()
            .when(active, |row| row.bg(theme.elevated_row_hover))
            .text_color(if active { theme.text } else { theme.text_muted })
            .on_click(cx.listener(move |modal, _: &ClickEvent, _, cx| modal.show(section, cx)))
            .child(label)
    }

    fn appearance(settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let choice = |name: ThemeName| {
            let (id, label) = match name {
                ThemeName::Dark => ("settings-theme-dark", "Dark"),
                ThemeName::Light => ("settings-theme-light", "Light"),
            };
            let chosen = settings.theme == name;
            div()
                .id(id)
                .debug_selector(|| id.to_owned())
                .px(px(8.))
                .border_1()
                .rounded(theme.elevated_radius)
                .border_color(if chosen {
                    theme.accent_outline
                } else {
                    theme.panel_border
                })
                .text_color(if chosen {
                    theme.accent
                } else {
                    theme.text_muted
                })
                .cursor_pointer()
                .on_click(cx.listener(move |modal, _: &ClickEvent, _, cx| {
                    modal.change(cx, |settings| settings.theme = name);
                }))
                .child(label)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(field(
                "Theme",
                theme,
                div()
                    .flex()
                    .gap(px(6.))
                    .child(choice(ThemeName::Dark))
                    .child(choice(ThemeName::Light)),
            ))
            .child(Self::stepper(Setting::UiScale, settings, theme, cx))
    }

    fn editor(&self, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let fonts = self.fonts.iter().enumerate().map(|(index, family)| {
            let id = format!("settings-font-{index}");
            let chosen = settings.font_family == family.as_ref();
            let pick = family.to_string();
            div()
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .px(px(6.))
                .rounded(theme.elevated_radius)
                .cursor_pointer()
                .hover(|row| row.bg(theme.elevated_row_hover))
                .text_color(if chosen { theme.accent } else { theme.text })
                .font_family(family.clone())
                .on_click(cx.listener(move |modal, _: &ClickEvent, _, cx| {
                    let pick = pick.clone();
                    modal.change(cx, |settings| settings.font_family = pick);
                }))
                .child(family.clone())
        });
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(field(
                "Font",
                theme,
                div()
                    .id("settings-fonts")
                    .h(px(140.) * (theme.font_size_hint / px(10.)))
                    .w_full()
                    .overflow_y_scroll()
                    .border_1()
                    .border_color(theme.panel_border)
                    .rounded(theme.elevated_radius)
                    .children(fonts),
            ))
            .child(Self::stepper(Setting::FontSize, settings, theme, cx))
            .child(Self::stepper(Setting::LineHeight, settings, theme, cx))
            .child(Self::stepper(Setting::ColumnWidth, settings, theme, cx))
    }

    /// A label, a "-" button, the value, and a "+" button.
    fn stepper(setting: Setting, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let (slug, label) = match setting {
            Setting::UiScale => ("ui-scale", "UI scale"),
            Setting::FontSize => ("font-size", "Font size"),
            Setting::LineHeight => ("line-height", "Line height"),
            Setting::ColumnWidth => ("column-width", "Column width"),
        };
        let value = settings.value(setting);
        let shown = match setting {
            Setting::UiScale => format!("{:.0}%", value * 100.),
            Setting::FontSize | Setting::ColumnWidth => format!("{value:.0} px"),
            Setting::LineHeight => format!("{value:.1}"),
        };
        let button = |up: bool| {
            let id = format!("settings-{slug}-{}", if up { "up" } else { "down" });
            div()
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .w(px(20.))
                .flex()
                .justify_center()
                .border_1()
                .border_color(theme.panel_border)
                .rounded(theme.elevated_radius)
                .cursor_pointer()
                .hover(|button| button.bg(theme.elevated_row_hover))
                .on_click(cx.listener(move |modal, _: &ClickEvent, _, cx| {
                    modal.change(cx, |settings| {
                        if up {
                            settings.increase(setting);
                        } else {
                            settings.decrease(setting);
                        }
                    });
                }))
                .child(if up { "+" } else { "-" })
        };
        field(
            label,
            theme,
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(button(false))
                .child(div().min_w(px(56.)).flex().justify_center().child(shown))
                .child(button(true)),
        )
    }
}

/// A field row: its label above its control.
fn field(label: &'static str, theme: &Theme, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(div().text_color(theme.text_muted).child(label))
        .child(control)
}

impl Focusable for SettingsModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsModal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let settings = cx.global::<SettingsStore>().settings.clone();
        let fields = match self.section {
            Section::Appearance => Self::appearance(&settings, &theme, cx),
            Section::Editor => self.editor(&settings, &theme, cx),
        };
        let scale = theme.font_size_hint / px(10.);
        div()
            .id("settings-backdrop")
            .key_context("SettingsModal")
            .track_focus(&self.focus_handle)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.background.opacity(0.6))
            .on_mouse_down(gpui::MouseButton::Left, |_: &MouseDownEvent, window, cx| {
                window.dispatch_action(Box::new(CloseSettings), cx);
            })
            .child(
                div()
                    .id("settings-modal")
                    .w(px(560.) * scale)
                    .h(px(380.) * scale)
                    .flex()
                    .bg(theme.elevated_background)
                    .border_1()
                    .border_color(theme.panel_border)
                    .rounded(theme.elevated_radius)
                    .text_size(theme.font_size_overflow)
                    .line_height(theme.line_height_overflow)
                    .text_color(theme.text)
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .w(px(140.) * scale)
                            .p(px(10.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .border_r_1()
                            .border_color(theme.panel_border)
                            .child(self.section_row(Section::Appearance, &theme, cx))
                            .child(self.section_row(Section::Editor, &theme, cx)),
                    )
                    .child(div().flex_1().p(px(16.)).child(fields)),
            )
    }
}

impl Workspace {
    /// Settings are open.
    #[must_use]
    pub fn settings_open(&self) -> bool {
        self.settings_open
    }

    /// Ctrl+, and the Chrome icon: opens Settings and gives them the keyboard.
    pub(crate) fn open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_open = true;
        window.focus(&self.settings_modal.focus_handle(cx));
        cx.notify();
    }

    /// Esc: closes Settings and gives the keyboard back to the Document.
    pub(crate) fn close_settings(
        &mut self,
        _: &CloseSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_open = false;
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
    }
}

/// The Chrome icon that opens Settings.
pub(crate) fn settings_button(theme: &Theme) -> impl IntoElement {
    div()
        .id("settings-button")
        .debug_selector(|| "settings-button".to_owned())
        .text_size(theme.font_size_hint)
        .text_color(theme.text_muted)
        .cursor_pointer()
        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx))
        .child("Settings")
}
