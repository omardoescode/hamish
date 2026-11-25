//! The Settings modal, Zed style: sections on the left, the fields of the
//! active section on the right. A change applies at once and saves to the settings file
//! on the background executor.

use gpui::{
    AnyElement, App, ClickEvent, Context, Corner, Div, EventEmitter, FocusHandle, Focusable,
    IntoElement, MouseButton, MouseDownEvent, Render, SharedString, Stateful, Window, anchored,
    deferred, div, prelude::*, px, rgb,
};
use ka_teb_core::{Setting, Settings, ThemeName, Toggle};

use crate::actions::{CloseSettings, OpenSettings};
use crate::settings::{self, SettingsStore};
use crate::{Theme, Workspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Appearance,
    Editor,
}

/// A select field: a button with the current choice that opens a list of choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Select {
    Theme,
    Font,
}

/// A settings save failed.
pub(crate) struct SaveFailed(pub(crate) String);

pub struct SettingsModal {
    focus_handle: FocusHandle,
    section: Section,
    /// The bundled font first, then the fonts the text system knows.
    fonts: Vec<SharedString>,
    /// The select whose list shows, if one does.
    open_select: Option<Select>,
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
            open_select: None,
        }
    }

    fn change(cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        let store = cx.global_mut::<SettingsStore>();
        change(&mut store.settings);
        let theme = Theme::new(&store.settings);
        cx.set_global(theme);
        cx.refresh_windows();
        let modal = cx.weak_entity();
        settings::save(cx, move |error, cx| {
            let failed = SaveFailed(error.to_string());
            modal.update(cx, |_, cx| cx.emit(failed)).ok();
        });
    }

    fn show(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.open_select = None;
        cx.notify();
    }

    fn toggle_select(&mut self, select: Select, cx: &mut Context<Self>) {
        self.open_select = (self.open_select != Some(select)).then_some(select);
        cx.notify();
    }

    fn close_select(&mut self, cx: &mut Context<Self>) {
        if self.open_select.take().is_some() {
            cx.notify();
        }
    }

    /// A select field: the button shows `current`; when open, a list of `rows` shows
    /// under it, over the fields below.
    fn select(
        &self,
        select: Select,
        current: impl IntoElement,
        rows: Vec<AnyElement>,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> Div {
        let (button_id, list_id) = match select {
            Select::Theme => ("settings-theme-select", "settings-theme-list"),
            Select::Font => ("settings-font-select", "settings-font-list"),
        };
        let open = self.open_select == Some(select);
        let scale = theme.font_size_hint / px(10.);
        let width = px(240.) * scale;
        let button = div()
            .id(button_id)
            .debug_selector(|| button_id.to_owned())
            .w(width)
            .px(px(8.))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .border_1()
            .rounded(theme.elevated_radius)
            .border_color(if open {
                theme.accent_outline
            } else {
                theme.panel_border
            })
            .cursor_pointer()
            .hover(|button| button.bg(theme.elevated_row_hover))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |modal, _: &ClickEvent, _, cx| {
                modal.toggle_select(select, cx);
            }))
            .child(div().overflow_hidden().whitespace_nowrap().child(current))
            .child(div().text_color(theme.text_muted).child(if open {
                "\u{25b4}"
            } else {
                "\u{25be}"
            }));
        let list = open.then(|| {
            deferred(
                anchored()
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id(list_id)
                            .occlude()
                            .mt(px(2.))
                            .w(width)
                            .max_h(px(220.) * scale)
                            .overflow_y_scroll()
                            .p(px(4.))
                            .bg(theme.elevated_background)
                            .border_1()
                            .border_color(theme.panel_border)
                            .rounded(theme.elevated_radius)
                            .shadow_md()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .children(rows),
                    ),
            )
        });
        div().flex().flex_col().child(button).children(list)
    }

    /// One choice in the list of a select; `chosen` shows in the accent color.
    fn select_row(
        id: String,
        chosen: bool,
        theme: &Theme,
        cx: &Context<Self>,
        change: impl Fn(&mut Settings) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id.clone())
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(theme.elevated_radius)
            .cursor_pointer()
            .hover(|row| row.bg(theme.elevated_row_hover))
            .text_color(if chosen { theme.accent } else { theme.text })
            .on_click(cx.listener(move |modal, _: &ClickEvent, _, cx| {
                modal.open_select = None;
                Self::change(cx, &change);
            }))
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

    fn appearance(&self, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let themes = ThemeName::ALL
            .into_iter()
            .map(|name| {
                Self::select_row(
                    format!("settings-theme-{}", name.slug()),
                    settings.theme == name,
                    theme,
                    cx,
                    move |settings| settings.theme = name,
                )
                .child(swatch(name, theme))
                .child(name.label())
                .into_any_element()
            })
            .collect();
        let current = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(swatch(settings.theme, theme))
            .child(settings.theme.label());
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(field(
                "Theme",
                theme,
                self.select(Select::Theme, current, themes, theme, cx),
            ))
            .child(Self::stepper(Setting::UiScale, settings, theme, cx))
            .child(Self::toggle(Toggle::ReducedMotion, settings, theme, cx))
            .child(Self::toggle(Toggle::FocusZoom, settings, theme, cx))
    }

    fn editor(&self, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let fonts = self
            .fonts
            .iter()
            .enumerate()
            .map(|(index, family)| {
                let pick = family.to_string();
                Self::select_row(
                    format!("settings-font-{index}"),
                    settings.font_family == family.as_ref(),
                    theme,
                    cx,
                    move |settings| settings.font_family.clone_from(&pick),
                )
                .font_family(family.clone())
                .child(family.clone())
                .into_any_element()
            })
            .collect();
        let current = div()
            .font_family(settings.font_family.clone())
            .child(settings.font_family.clone());
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(field(
                "Font",
                theme,
                self.select(Select::Font, current, fonts, theme, cx),
            ))
            .child(Self::stepper(Setting::FontSize, settings, theme, cx))
            .child(Self::stepper(Setting::LineHeight, settings, theme, cx))
            .child(Self::stepper(Setting::ColumnWidth, settings, theme, cx))
            .child(Self::toggle(Toggle::CycleSounds, settings, theme, cx))
    }

    /// A label and one button that shows "On" or "Off" and flips the setting.
    fn toggle(toggle: Toggle, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let on = settings.is_on(toggle);
        let id = format!("settings-{}", toggle.slug());
        field(
            toggle.label(),
            theme,
            div().flex().child(
                div()
                    .id(SharedString::from(id.clone()))
                    .debug_selector(move || id.clone())
                    .px(px(8.))
                    .border_1()
                    .rounded(theme.elevated_radius)
                    .border_color(if on {
                        theme.accent_outline
                    } else {
                        theme.panel_border
                    })
                    .text_color(if on { theme.accent } else { theme.text_muted })
                    .cursor_pointer()
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        Self::change(cx, |settings| settings.flip(toggle));
                    }))
                    .child(settings.shown_toggle(toggle)),
            ),
        )
    }

    /// A label, a "-" button, the value, and a "+" button.
    fn stepper(setting: Setting, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let button = |up: bool| {
            let id = format!(
                "settings-{}-{}",
                setting.slug(),
                if up { "up" } else { "down" }
            );
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
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    Self::change(cx, |settings| {
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
            setting.label(),
            theme,
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(button(false))
                .child(
                    div()
                        .min_w(px(56.))
                        .flex()
                        .justify_center()
                        .child(settings.shown(setting)),
                )
                .child(button(true)),
        )
    }
}

/// A small square in the background color of `name` with a dot in its accent color.
fn swatch(name: ThemeName, theme: &Theme) -> Div {
    let palette = name.palette();
    div()
        .size(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(2.))
        .border_1()
        .border_color(theme.panel_border)
        .bg(rgb(palette.background))
        .child(div().size(px(4.)).rounded_full().bg(rgb(palette.accent)))
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
            Section::Appearance => self.appearance(&settings, &theme, cx),
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
                    .h(px(440.) * scale)
                    .flex()
                    .bg(theme.elevated_background)
                    .border_1()
                    .border_color(theme.panel_border)
                    .rounded(theme.elevated_radius)
                    .text_size(theme.font_size_overflow)
                    .line_height(theme.line_height_overflow)
                    .text_color(theme.text)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|modal, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            modal.close_select(cx);
                        }),
                    )
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
