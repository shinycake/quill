//! App themes, after Telegram Desktop's (`window/themes/*`): the four
//! built-in themes with their accent colours, custom themes stored as
//! `.tdesktop-theme` files, and the palette Quill paints with.
//!
//! Cloud themes (tdesktop `data_cloud_themes.cpp`, `account.getThemes`)
//! are not here: TDLib has no API for them.
#![deny(clippy::indexing_slicing)]

pub mod embedded;
pub mod file;
pub mod keys;
pub mod omarchy;
pub mod palette;
pub mod png;
pub mod store;

pub use embedded::BuiltinTheme;
pub use file::{ThemeFile, ThemeFileError};
pub use keys::{KeyGroup, PaletteKey};
pub use palette::{Palette, ThemeBackground, hex_string, parse_hex};

use serde::{Deserialize, Serialize};

const OMARCHY_ID: &str = "omarchy";

/// Which theme: a built-in one or a custom theme by name. Stored in the
/// appearance prefs as `"classic"`, `"day"`, `"tinted"`, `"night"` or
/// `"custom:<name>"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ThemeId {
    Builtin(BuiltinTheme),
    Custom(String),
    /// Omarchy's current theme, followed while Quill runs on Omarchy.
    Omarchy,
}

impl ThemeId {
    /// The day default: Quill Light (what Quill always showed).
    pub fn default_day() -> Self {
        ThemeId::Builtin(BuiltinTheme::QuillLight)
    }

    /// The night default: Quill Dark.
    pub fn default_night() -> Self {
        ThemeId::Builtin(BuiltinTheme::QuillDark)
    }

    pub fn builtin(&self) -> Option<BuiltinTheme> {
        match self {
            ThemeId::Builtin(theme) => Some(*theme),
            ThemeId::Custom(_) | ThemeId::Omarchy => None,
        }
    }

    /// The name shown in the settings.
    pub fn display_name(&self) -> &str {
        match self {
            ThemeId::Builtin(theme) => theme.name(),
            ThemeId::Custom(name) => name,
            ThemeId::Omarchy => "Omarchy",
        }
    }
}

impl From<String> for ThemeId {
    /// Lenient, so a hand-edited value never resets the other prefs: an
    /// unknown id is read as a custom theme name (and a missing file then
    /// falls back to the built-in default).
    fn from(value: String) -> Self {
        if let Some(theme) = BuiltinTheme::from_id(&value) {
            return ThemeId::Builtin(theme);
        }
        if value == OMARCHY_ID {
            return ThemeId::Omarchy;
        }
        let name = value.strip_prefix("custom:").unwrap_or(&value);
        ThemeId::Custom(store::clean_name(name))
    }
}

impl From<ThemeId> for String {
    fn from(id: ThemeId) -> Self {
        match id {
            ThemeId::Builtin(theme) => theme.id().to_string(),
            ThemeId::Custom(name) => format!("custom:{name}"),
            ThemeId::Omarchy => OMARCHY_ID.to_string(),
        }
    }
}

/// The palette for a custom theme file: keys it leaves out come from the
/// default (Classic) palette, as in tdesktop.
pub fn custom_palette(file: &ThemeFile, background: Option<u32>) -> Palette {
    file.palette(&BuiltinTheme::Classic.palette(), background)
}

#[cfg(test)]
mod tests;
