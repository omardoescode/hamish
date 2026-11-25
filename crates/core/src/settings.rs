//! Settings (issue #21): theme, UI scale, editor font values, cycle sounds (issue #27),
//! and reduced motion (issue #31), with their defaults, limits, and the JSON file in the
//! XDG config dir.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The two themes. Dark is the spec "Theme Mapping" table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    #[default]
    Dark,
    Light,
}

/// A numeric setting the writer steps up and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Scale of the Chrome and panels; 1.0 is the spec size.
    UiScale,
    /// Body font size in pixels.
    FontSize,
    /// Body line height as a multiple of the font size.
    LineHeight,
    /// Text column width in pixels.
    ColumnWidth,
}

impl Setting {
    /// Lowest value, highest value, and the step.
    const fn limits(self) -> (f32, f32, f32) {
        match self {
            Self::UiScale => (0.5, 2.0, 0.1),
            Self::FontSize => (8.0, 40.0, 1.0),
            Self::LineHeight => (1.0, 3.0, 0.1),
            Self::ColumnWidth => (320.0, 1600.0, 20.0),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::UiScale => "ui_scale",
            Self::FontSize => "font_size",
            Self::LineHeight => "line_height",
            Self::ColumnWidth => "column_width",
        }
    }

    const ALL: [Self; 4] = [
        Self::UiScale,
        Self::FontSize,
        Self::LineHeight,
        Self::ColumnWidth,
    ];
}

/// The writer's settings. The file holds them as JSON; a field not in the file takes
/// its default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeName,
    pub ui_scale: f32,
    /// Body font family; the bundled IBM Plex Mono by default (ADR 0023).
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub column_width: f32,
    /// Effects are instant (spec NFR "Reduced motion"). Off by default: GPUI 0.2.2 does not
    /// expose the OS setting.
    pub reduced_motion: bool,
    /// A tone on each Active variant switch and a sound on a Variant delete (issue #27).
    pub cycle_sounds: bool,
}

impl Default for Settings {
    /// The spec "Theme Mapping" values: 15 px body font on 27 px lines, a 585 px column.
    fn default() -> Self {
        Self {
            theme: ThemeName::Dark,
            ui_scale: 1.0,
            font_family: "IBM Plex Mono".to_owned(),
            font_size: 15.0,
            line_height: 1.8,
            column_width: 585.0,
            reduced_motion: false,
            cycle_sounds: true,
        }
    }
}

/// The settings a load gives, and why it fell back to defaults, if it did.
#[derive(Debug)]
pub struct LoadedSettings {
    pub settings: Settings,
    pub problem: Option<SettingsError>,
}

/// Why the settings file did not load or save.
#[derive(Debug)]
pub enum SettingsError {
    Io {
        path: PathBuf,
        error: io::Error,
    },
    /// Not settings JSON, or a value out of its limits.
    Bad {
        path: PathBuf,
        reason: String,
    },
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Bad { path, reason } => write!(f, "{}: {reason}", path.display()),
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error, .. } => Some(error),
            Self::Bad { .. } => None,
        }
    }
}

/// `$XDG_CONFIG_HOME/ka-teb/settings.json`; `~/.config` when `XDG_CONFIG_HOME` is not an
/// absolute path (XDG Base Directory spec). `None` without either.
#[must_use]
pub fn settings_path(xdg_config_home: Option<&OsStr>, home: Option<&Path>) -> Option<PathBuf> {
    let config = xdg_config_home
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .map(Path::to_owned)
        .or_else(|| home.map(|home| home.join(".config")))?;
    Some(config.join("ka-teb").join("settings.json"))
}

impl Settings {
    /// Reads the settings file. A missing file gives the defaults; a file that does not
    /// read gives the defaults and a problem; a value out of its limits gives that
    /// value's default and a problem.
    #[must_use]
    pub fn load(path: &Path) -> LoadedSettings {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return LoadedSettings {
                    settings: Self::default(),
                    problem: None,
                };
            }
            Err(error) => {
                return LoadedSettings {
                    settings: Self::default(),
                    problem: Some(SettingsError::Io {
                        path: path.to_owned(),
                        error,
                    }),
                };
            }
        };
        let mut settings: Self = match serde_json::from_str(&text) {
            Ok(settings) => settings,
            Err(error) => {
                return LoadedSettings {
                    settings: Self::default(),
                    problem: Some(SettingsError::Bad {
                        path: path.to_owned(),
                        reason: error.to_string(),
                    }),
                };
            }
        };
        let bad = settings.reset_out_of_limits();
        let problem = (!bad.is_empty()).then(|| SettingsError::Bad {
            path: path.to_owned(),
            reason: format!("out of limits, default used: {}", bad.join(", ")),
        });
        LoadedSettings { settings, problem }
    }

    /// Writes the settings file and its directory. A new file replaces the old one in one
    /// step, so a failed write leaves the old file.
    ///
    /// # Errors
    /// When the directory or the file does not write.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let io_error = |error| SettingsError::Io {
            path: path.to_owned(),
            error,
        };
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory).map_err(io_error)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|error| SettingsError::Bad {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
        let mut next = OsString::from(path);
        next.push(".new");
        fs::write(&next, json).map_err(io_error)?;
        fs::rename(&next, path).map_err(io_error)
    }

    #[must_use]
    pub fn value(&self, setting: Setting) -> f32 {
        match setting {
            Setting::UiScale => self.ui_scale,
            Setting::FontSize => self.font_size,
            Setting::LineHeight => self.line_height,
            Setting::ColumnWidth => self.column_width,
        }
    }

    fn value_mut(&mut self, setting: Setting) -> &mut f32 {
        match setting {
            Setting::UiScale => &mut self.ui_scale,
            Setting::FontSize => &mut self.font_size,
            Setting::LineHeight => &mut self.line_height,
            Setting::ColumnWidth => &mut self.column_width,
        }
    }

    /// One step up, to at most the highest value.
    pub fn increase(&mut self, setting: Setting) {
        self.step(setting, 1.0);
    }

    /// One step down, to at least the lowest value.
    pub fn decrease(&mut self, setting: Setting) {
        self.step(setting, -1.0);
    }

    fn step(&mut self, setting: Setting, direction: f32) {
        let (low, high, step) = setting.limits();
        let value = self.value_mut(setting);
        let next = (*value + direction * step).clamp(low, high);
        *value = (next * 10.0).round() / 10.0;
    }

    /// Puts each value outside its limits back to its default; returns their names.
    fn reset_out_of_limits(&mut self) -> Vec<&'static str> {
        let defaults = Self::default();
        let mut bad = Vec::new();
        for setting in Setting::ALL {
            let (low, high, _) = setting.limits();
            if !(low..=high).contains(&self.value(setting)) {
                *self.value_mut(setting) = defaults.value(setting);
                bad.push(setting.name());
            }
        }
        if self.font_family.trim().is_empty() {
            self.font_family = defaults.font_family;
            bad.push("font_family");
        }
        bad
    }
}
