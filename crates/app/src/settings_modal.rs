//! The Settings modal, Zed style: sections on the left, the fields of the
//! active section on the right. A change applies at once and saves to the settings file
//! on the background executor.

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Anchor, Animation, AnimationExt, AnyElement, App, Bounds, ClickEvent, Context, Div,
    EventEmitter, FocusHandle, Focusable, Hsla, IntoElement, KeyDownEvent, Modifiers, MouseButton,
    MouseDownEvent, PathBuilder, Pixels, Render, ScrollStrategy, SharedString, Stateful,
    UniformListScrollHandle, Window, anchored, canvas, deferred, div, point, prelude::*, px, rgb,
    uniform_list,
};
use hamish_core::{
    Direction, Effect, RowCommand, SelectSearch, Setting, Settings, SettingsCommand, ThemeName,
    Toggle, caret_shown,
};

use crate::actions::{
    CloseSelect, CloseSettings, DeleteQueryBackward, NextRow, OpenSettings, PickRow, PreviousRow,
};
use crate::caret::CARET_WIDTH;
use crate::chrome::{
    clickable_row, elevated_surface, popover_surface, selectable_row, text_button,
};
use crate::editor::{vim_key, vim_on};
use crate::fonts;
use crate::settings::{self, SettingsStore};
use crate::vim_rows::{VimRows, run_row_command};
use crate::workspace::Visibility;
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
    UiFont,
    TextFont,
}

/// One field of Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Select(Select),
    Stepper(Setting),
    Toggle(Toggle),
}

/// The fields of Settings in the order they show, with the section of each. Vim mode `j`
/// and `k` go through them in this order.
const FIELDS: [(Section, Field); 13] = [
    (Section::Appearance, Field::Select(Select::Theme)),
    (Section::Appearance, Field::Select(Select::UiFont)),
    (Section::Appearance, Field::Stepper(Setting::UiScale)),
    (Section::Appearance, Field::Toggle(Toggle::ReducedMotion)),
    (Section::Appearance, Field::Toggle(Toggle::FocusZoom)),
    (Section::Editor, Field::Select(Select::TextFont)),
    (Section::Editor, Field::Stepper(Setting::FontSize)),
    (Section::Editor, Field::Stepper(Setting::LineHeight)),
    (Section::Editor, Field::Stepper(Setting::ColumnWidth)),
    (Section::Editor, Field::Toggle(Toggle::CycleSounds)),
    (Section::Editor, Field::Toggle(Toggle::VimMode)),
    (Section::Editor, Field::Toggle(Toggle::Markers)),
    (Section::Editor, Field::Toggle(Toggle::MarkdownStyles)),
];

/// A settings save failed.
pub(crate) struct SaveFailed(pub(crate) String);

pub struct SettingsModal {
    focus_handle: FocusHandle,
    section: Section,
    /// The bundled font first, then the fonts the text system knows. Empty until a font
    /// select opens.
    fonts: Arc<[SharedString]>,
    /// The select whose list shows, if one does.
    open_select: Option<Select>,
    /// The search field of the open select has this focus.
    search_focus: FocusHandle,
    search: SelectSearch,
    scroll: UniformListScrollHandle,
    /// The field that Vim mode keys act on: an index into [`FIELDS`].
    vim_field: usize,
}

impl EventEmitter<SaveFailed> for SettingsModal {}

impl SettingsModal {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            section: Section::Appearance,
            fonts: Arc::from([]),
            open_select: None,
            search_focus: cx.focus_handle(),
            search: SelectSearch::default(),
            scroll: UniformListScrollHandle::new(),
            vim_field: 0,
        }
    }

    fn change(cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        let modal = cx.weak_entity();
        settings::change(cx, change, move |error, cx| {
            let failed = SaveFailed(error.to_string());
            modal.update(cx, |_, cx| cx.emit(failed)).ok();
        });
    }

    fn show(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.section = section;
        self.vim_field = FIELDS
            .iter()
            .position(|(shown, _)| *shown == section)
            .unwrap_or_default();
        self.close_select(window, cx);
        cx.notify();
    }

    /// Opens `select` with an empty search and gives the keyboard to its search field, or
    /// closes it when it is open.
    fn toggle_select(&mut self, select: Select, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_select == Some(select) {
            self.close_select(window, cx);
            return;
        }
        if select != Select::Theme {
            self.fonts = fonts::font_names_now(cx);
        }
        self.open_select = Some(select);
        self.search = SelectSearch::default();
        self.scroll = UniformListScrollHandle::new();
        window.focus(&self.search_focus, cx);
        cx.notify();
    }

    /// Closes the open select and gives the keyboard back to the modal.
    pub(crate) fn close_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_select.take().is_some() {
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    /// The rows of `select` before the search: the theme names, or the fonts. The Text
    /// font rows start with "Default", the UI font.
    fn names(&self, select: Select) -> Rc<[SharedString]> {
        match select {
            Select::Theme => ThemeName::ALL
                .into_iter()
                .map(|name| SharedString::from(name.label()))
                .collect(),
            Select::UiFont => self.fonts.iter().cloned().collect(),
            Select::TextFont => std::iter::once(SharedString::from("Default"))
                .chain(self.fonts.iter().cloned())
                .collect(),
        }
    }

    /// The indexes into [`Self::names`] of the rows that the search keeps.
    fn matches(&self, select: Select) -> Vec<usize> {
        let names = self.names(select);
        self.search.matches(names.iter().map(AsRef::as_ref))
    }

    /// Row `index` of [`Self::names`] becomes the setting of `select`; the select closes.
    fn pick(&mut self, select: Select, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.names(select).get(index).map(ToString::to_string) else {
            return;
        };
        let theme = ThemeName::ALL.get(index).copied();
        self.close_select(window, cx);
        Self::change(cx, move |settings| match select {
            Select::Theme => settings.theme = theme.unwrap_or(settings.theme),
            Select::UiFont => settings.ui_font = name,
            Select::TextFont => settings.text_font = (index > 0).then_some(name),
        });
    }

    fn previous_row(&mut self, _: &PreviousRow, _: &mut Window, cx: &mut Context<Self>) {
        self.search.move_up();
        self.scroll
            .scroll_to_item(self.search.row(), ScrollStrategy::Top);
        cx.notify();
    }

    fn next_row(&mut self, _: &NextRow, _: &mut Window, cx: &mut Context<Self>) {
        let Some(select) = self.open_select else {
            return;
        };
        self.search.move_down(self.matches(select).len());
        self.scroll
            .scroll_to_item(self.search.row(), ScrollStrategy::Top);
        cx.notify();
    }

    fn pick_row(&mut self, _: &PickRow, window: &mut Window, cx: &mut Context<Self>) {
        let Some(select) = self.open_select else {
            return;
        };
        if let Some(index) = self.matches(select).get(self.search.row()).copied() {
            self.pick(select, index, window, cx);
        }
    }

    fn close_select_key(&mut self, _: &CloseSelect, window: &mut Window, cx: &mut Context<Self>) {
        self.close_select(window, cx);
    }

    fn delete_query_backward(
        &mut self,
        _: &DeleteQueryBackward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.delete_backward();
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// In Vim mode `j` and `k` move between the fields, `h` and `l` change a value, and
    /// Enter opens a select or flips a toggle.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !vim_on(cx) || self.open_select.is_some() {
            return;
        }
        match SettingsCommand::of(vim_key(&event.keystroke)) {
            SettingsCommand::PassOn => return,
            SettingsCommand::Nothing => {}
            SettingsCommand::Row(command) => run_row_command(self, command, window, cx),
            SettingsCommand::Step(direction) => Self::step(self.vim_field(), direction, cx),
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// The field that the Vim mode keys act on.
    fn vim_field(&self) -> Option<Field> {
        FIELDS.get(self.vim_field).map(|(_, field)| *field)
    }

    /// `h` and `l`: a lower or higher value of a stepper; a toggle turns off or on.
    fn step(field: Option<Field>, direction: Direction, cx: &mut Context<Self>) {
        match field {
            Some(Field::Stepper(setting)) => {
                Self::change(cx, move |settings| settings.step(setting, direction));
            }
            Some(Field::Toggle(toggle)) => {
                let on = direction == Direction::Forward;
                Self::change(cx, move |settings| settings.set(toggle, on));
            }
            Some(Field::Select(_)) | None => {}
        }
    }

    /// Typed text goes into the search of the open select. In Vim mode `j` and `k` move
    /// between its rows.
    fn on_search_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let command = vim_on(cx)
            .then(|| RowCommand::in_select(vim_key(keystroke)))
            .flatten();
        match command {
            Some(RowCommand::Down) => self.next_row(&NextRow, window, cx),
            Some(RowCommand::Up) => self.previous_row(&PreviousRow, window, cx),
            Some(RowCommand::Enter | RowCommand::Close) | None => {
                return self.type_search(event, cx);
            }
        }
        cx.stop_propagation();
    }

    fn type_search(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let Some(typed) = keystroke.key_char.as_ref() else {
            return;
        };
        if !keystroke.modifiers.is_subset_of(&Modifiers::shift()) {
            return;
        }
        self.search.type_text(typed);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.stop_propagation();
        cx.notify();
    }

    /// A select field: the button shows `current`; when open, the search field and the rows
    /// show under it, over the fields below.
    fn select(
        &self,
        select: Select,
        current: impl IntoElement,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> Div {
        let (button_id, list_id) = match select {
            Select::Theme => ("settings-theme-select", "settings-theme-list"),
            Select::UiFont => ("settings-ui-font-select", "settings-ui-font-list"),
            Select::TextFont => ("settings-text-font-select", "settings-text-font-list"),
        };
        let open = self.open_select == Some(select);
        let reduced_motion = settings::is_on(cx, |settings| settings.reduced_motion);
        let width = list_width(theme);
        let outline = if open {
            theme.accent_outline
        } else {
            theme.panel_border
        };
        let button = text_button(theme, outline)
            .id(button_id)
            .debug_selector(|| button_id.to_owned())
            .w(width)
            .px(px(8.))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |modal, _: &ClickEvent, window, cx| {
                modal.toggle_select(select, window, cx);
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
                    .anchor(Anchor::TopLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        popover_surface(theme)
                            .id(list_id)
                            .track_focus(&self.search_focus)
                            .key_context("SettingsSelect")
                            .on_action(cx.listener(Self::previous_row))
                            .on_action(cx.listener(Self::next_row))
                            .on_action(cx.listener(Self::pick_row))
                            .on_action(cx.listener(Self::close_select_key))
                            .on_action(cx.listener(Self::delete_query_backward))
                            .on_key_down(cx.listener(Self::on_search_key_down))
                            .occlude()
                            .mt(px(2.))
                            .w(width)
                            .p(px(4.))
                            .border_1()
                            .border_color(theme.panel_border)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(self.search_field(theme, reduced_motion))
                            .child(self.rows(select, theme, cx)),
                    ),
            )
        });
        div().flex().flex_col().child(button).children(list)
    }

    /// The search field of the open select: a search icon, the query, and a caret at its end.
    /// While the query is empty, "Search" shows after the caret in the placeholder color.
    fn search_field(&self, theme: &Theme, reduced_motion: bool) -> Div {
        let query = self.search.query();
        let shown_query = (!query.is_empty()).then(|| {
            div()
                .debug_selector(|| "settings-select-query".to_owned())
                .child(SharedString::from(query.to_owned()))
        });
        let placeholder = query.is_empty().then(|| {
            div()
                .debug_selector(|| "settings-select-placeholder".to_owned())
                .text_color(theme.text_placeholder)
                .child("Search")
        });
        div()
            .debug_selector(|| "settings-select-search".to_owned())
            .h(row_height(theme))
            .mb(px(4.))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .border_b_1()
            .border_color(theme.panel_border)
            .text_color(theme.text)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(search_icon(theme.text_placeholder))
            .child(
                div()
                    .flex()
                    .items_center()
                    .children(shown_query)
                    .child(search_caret(query.len(), theme, reduced_motion))
                    .children(placeholder),
            )
    }

    /// The rows of the open select that the search keeps: only the rows in view are made,
    /// all at one height.
    fn rows(&self, select: Select, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let names = self.names(select);
        let matches: Rc<[usize]> = self.search.matches(names.iter().map(AsRef::as_ref)).into();
        let count = matches.len();
        if count == 0 {
            return div()
                .debug_selector(|| "settings-select-no-match".to_owned())
                .h(row_height(theme))
                .px(px(6.))
                .flex()
                .items_center()
                .text_color(theme.text_muted)
                .child("No match")
                .into_any_element();
        }
        let rows_height = row_height(theme) * f32::from(u16::try_from(count).unwrap_or(u16::MAX));
        let list_id = match select {
            Select::Theme => "settings-theme-rows",
            Select::UiFont => "settings-ui-font-rows",
            Select::TextFont => "settings-text-font-rows",
        };
        let keyboard_row = self.search.row();
        uniform_list(
            list_id,
            count,
            cx.processor(move |_, range: Range<usize>, _, cx| {
                let settings = cx.global::<SettingsStore>().settings.clone();
                let theme = cx.global::<Theme>().clone();
                range
                    .filter_map(|row| {
                        let index = *matches.get(row)?;
                        let name = names.get(index)?.clone();
                        let row = SelectRow {
                            select,
                            index,
                            name,
                            keyboard: row == keyboard_row,
                        };
                        Some(row.render(&settings, &theme, cx))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .debug_selector(|| "settings-select-rows".to_owned())
        .track_scroll(&self.scroll)
        .w_full()
        .h(rows_height.min(list_max_height(theme)))
        .into_any_element()
    }

    fn section_row(&self, section: Section, theme: &Theme, cx: &Context<Self>) -> Stateful<Div> {
        let (id, label) = match section {
            Section::Appearance => ("settings-section-appearance", "Appearance"),
            Section::Editor => ("settings-section-editor", "Editor"),
        };
        let active = self.section == section;
        selectable_row(theme, active)
            .id(id)
            .debug_selector(|| id.to_owned())
            .px(px(8.))
            .py(px(4.))
            .cursor_pointer()
            .text_color(if active { theme.text } else { theme.text_muted })
            .on_click(cx.listener(move |modal, _: &ClickEvent, window, cx| {
                modal.show(section, window, cx);
            }))
            .child(label)
    }

    /// The fields of the active section. In Vim mode the field of the keys shows the row
    /// hover background.
    fn fields(&self, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let vim_field = vim_on(cx).then_some(self.vim_field);
        div().flex().flex_col().gap(px(4.)).children(
            FIELDS
                .iter()
                .enumerate()
                .filter(|(_, (section, _))| *section == self.section)
                .map(|(index, (_, field))| {
                    let shown = match *field {
                        Field::Select(select) => self.select_field(select, settings, theme, cx),
                        Field::Stepper(setting) => Self::stepper(setting, settings, theme, cx),
                        Field::Toggle(toggle) => Self::toggle(toggle, settings, theme, cx),
                    };
                    selectable_row(theme, vim_field == Some(index))
                        .px(px(4.))
                        .py(px(4.))
                        .child(shown)
                }),
        )
    }

    /// A select field with its label; the button shows the current choice.
    fn select_field(
        &self,
        select: Select,
        settings: &Settings,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> Div {
        let (label, current) = match select {
            Select::Theme => (
                "Theme",
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(swatch(settings.theme, theme))
                    .child(settings.theme.label()),
            ),
            Select::UiFont => ("UI font", div().child(settings.ui_font.clone())),
            Select::TextFont => (
                "Text font",
                div().child(
                    settings
                        .text_font
                        .clone()
                        .unwrap_or_else(|| "Default".to_owned()),
                ),
            ),
        };
        field(label, theme, self.select(select, current, theme, cx))
    }

    /// A label and one button that shows "On" or "Off" and flips the setting.
    fn toggle(toggle: Toggle, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let on = settings.is_on(toggle);
        let outline = if on {
            theme.accent_outline
        } else {
            theme.panel_border
        };
        let id = format!("settings-{}", toggle.slug());
        field(
            toggle.label(),
            theme,
            div().flex().child(
                text_button(theme, outline)
                    .id(SharedString::from(id.clone()))
                    .debug_selector(move || id.clone())
                    .px(px(8.))
                    .text_color(if on { theme.accent } else { theme.text_muted })
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        Self::change(cx, |settings| settings.flip(toggle));
                    }))
                    .child(settings.shown_toggle(toggle)),
            ),
        )
    }

    /// A label, a "-" button, the value, and a "+" button.
    fn stepper(setting: Setting, settings: &Settings, theme: &Theme, cx: &Context<Self>) -> Div {
        let button = |direction: Direction| {
            let (name, sign) = match direction {
                Direction::Forward => ("up", "+"),
                Direction::Back => ("down", "-"),
            };
            let id = format!("settings-{}-{name}", setting.slug());
            text_button(theme, theme.panel_border)
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .w(px(20.))
                .flex()
                .justify_center()
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    Self::change(cx, move |settings| settings.step(setting, direction));
                }))
                .child(sign)
        };
        field(
            setting.label(),
            theme,
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(button(Direction::Back))
                .child(
                    div()
                        .min_w(px(56.))
                        .flex()
                        .justify_center()
                        .child(settings.shown(setting)),
                )
                .child(button(Direction::Forward)),
        )
    }
}

/// One row of an open select: row `index` of its names.
struct SelectRow {
    select: Select,
    index: usize,
    name: SharedString,
    /// Up, Down, and Enter act on this row.
    keyboard: bool,
}

impl SelectRow {
    fn id(&self) -> String {
        let index = self.index;
        match self.select {
            Select::Theme => format!(
                "settings-theme-{}",
                ThemeName::ALL.get(index).map_or("", |name| name.slug())
            ),
            Select::UiFont => format!("settings-ui-font-{index}"),
            Select::TextFont => index.checked_sub(1).map_or_else(
                || "settings-text-font-default".to_owned(),
                |font| format!("settings-text-font-{font}"),
            ),
        }
    }

    fn chosen(&self, settings: &Settings) -> bool {
        match self.select {
            Select::Theme => ThemeName::ALL.get(self.index) == Some(&settings.theme),
            Select::UiFont => settings.ui_font == self.name.as_ref(),
            Select::TextFont => {
                settings.text_font.as_deref() == (self.index > 0).then_some(self.name.as_ref())
            }
        }
    }

    /// The row; `chosen` shows in the accent color, the keyboard row with the row hover
    /// background.
    fn render(self, settings: &Settings, theme: &Theme, cx: &Context<SettingsModal>) -> AnyElement {
        let id = self.id();
        let (select, index) = (self.select, self.index);
        let swatch = ThemeName::ALL
            .get(index)
            .filter(|_| select == Select::Theme)
            .map(|name| swatch(*name, theme));
        clickable_row(theme)
            .when(self.keyboard, |row| row.bg(theme.elevated_row_hover))
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id.clone())
            .h(row_height(theme))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_color(if self.chosen(settings) {
                theme.accent
            } else {
                theme.text
            })
            .on_click(cx.listener(move |modal, _: &ClickEvent, window, cx| {
                modal.pick(select, index, window, cx);
            }))
            .children(swatch)
            .child(div().overflow_hidden().whitespace_nowrap().child(self.name))
            .into_any_element()
    }
}

/// A magnifier in `color`: a ring with a handle to the lower right.
fn search_icon(color: Hsla) -> Div {
    let paint = move |bounds: Bounds<Pixels>, (), window: &mut Window, _: &mut App| {
        let radius = px(3.);
        let centre = bounds.origin + point(px(4.), px(4.));
        let mut magnifier = PathBuilder::stroke(px(1.2));
        magnifier.move_to(centre + point(radius, px(0.)));
        magnifier.arc_to(
            point(radius, radius),
            px(0.),
            false,
            true,
            centre - point(radius, px(0.)),
        );
        magnifier.arc_to(
            point(radius, radius),
            px(0.),
            false,
            true,
            centre + point(radius, px(0.)),
        );
        magnifier.move_to(centre + point(px(2.2), px(2.2)));
        magnifier.line_to(bounds.origin + point(px(9.5), px(9.5)));
        if let Ok(path) = magnifier.build() {
            window.paint_path(path, color);
        }
    };
    div()
        .debug_selector(|| "settings-select-search-icon".to_owned())
        .flex_none()
        .size(px(10.))
        .child(canvas(|_, _, _| (), paint).size_full())
}

/// The caret at the end of the search query, in the Document caret look. It blinks, and its
/// blink starts again when `query_len` changes. With `reduced_motion` it stays on.
fn search_caret(query_len: usize, theme: &Theme, reduced_motion: bool) -> AnyElement {
    let caret = div()
        .flex_none()
        .w(CARET_WIDTH)
        .h(theme.line_height_overflow)
        .bg(theme.accent);
    if reduced_motion {
        return caret
            .debug_selector(|| "settings-select-caret-still".to_owned())
            .into_any_element();
    }
    let round = Effect::CaretBlink.duration();
    caret
        .debug_selector(|| "settings-select-caret".to_owned())
        .with_animation(
            ("settings-select-caret", query_len),
            Animation::new(round).repeat(),
            move |caret, delta| {
                let shown = caret_shown(round.mul_f32(delta), false);
                caret.opacity(if shown { 1. } else { 0. })
            },
        )
        .into_any_element()
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

/// Width of a select button and its list.
fn list_width(theme: &Theme) -> Pixels {
    px(240.) * (theme.font_size_hint / px(10.))
}

/// Highest a select list grows before it scrolls.
fn list_max_height(theme: &Theme) -> Pixels {
    px(212.) * (theme.font_size_hint / px(10.))
}

/// Height of each row of a select list.
fn row_height(theme: &Theme) -> Pixels {
    theme.line_height_overflow + px(4.)
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

impl VimRows for SettingsModal {
    fn vim_row(&self, _: &App) -> usize {
        self.vim_field
    }

    /// The section of the field shows.
    fn select_vim_row(&mut self, row: usize, _: &mut Context<Self>) {
        self.vim_field = row.min(FIELDS.len() - 1);
        if let Some((section, _)) = FIELDS.get(self.vim_field) {
            self.section = *section;
        }
    }

    /// Enter opens a select or flips a toggle.
    fn enter_vim_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.vim_field() {
            Some(Field::Select(select)) => self.toggle_select(select, window, cx),
            Some(Field::Toggle(toggle)) => Self::change(cx, move |settings| settings.flip(toggle)),
            Some(Field::Stepper(_)) | None => {}
        }
    }

    fn close_vim_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.dispatch_action(Box::new(CloseSettings), cx);
    }
}

impl Render for SettingsModal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let settings = cx.global::<SettingsStore>().settings.clone();
        let fields = self.fields(&settings, &theme, cx);
        let scale = theme.font_size_hint / px(10.);
        div()
            .id("settings-backdrop")
            .key_context("SettingsModal")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
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
                elevated_surface(&theme)
                    .id("settings-modal")
                    .w(px(560.) * scale)
                    .h(px(440.) * scale)
                    .flex()
                    .border_1()
                    .border_color(theme.panel_border)
                    .text_size(theme.font_size_overflow)
                    .line_height(theme.line_height_overflow)
                    .text_color(theme.text)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|modal, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            modal.close_select(window, cx);
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
        self.settings_shown.is_shown()
    }

    /// Ctrl+, and the Chrome icon: opens Settings and gives them the keyboard.
    pub(crate) fn open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_shown = Visibility::Shown;
        self.settings_modal
            .update(cx, |modal, cx| modal.close_select(window, cx));
        window.focus(&self.settings_modal.focus_handle(cx), cx);
        cx.notify();
    }

    /// Esc: closes Settings and gives the keyboard back to the Document.
    pub(crate) fn close_settings(
        &mut self,
        _: &CloseSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_shown = Visibility::Hidden;
        window.focus(&self.editor.focus_handle(cx), cx);
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
