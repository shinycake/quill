//! Appearance page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Appearance;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("appearance.themes", P, keys::Q_MISC_THEMES).keywords(&["color scheme", "skin"]),
    Entry::new("appearance.night_mode", P, keys::MENU_NIGHT_MODE)
        .keywords(&["dark", "theme", "light"]),
    Entry::new("appearance.auto_night", P, keys::Q_PREFS_AUTO_NIGHT).keywords(&[
        "schedule",
        "sunset",
        "system theme",
    ]),
    Entry::new("appearance.accent_color", P, keys::Q_MISC_ACCENT_COLOR)
        .keywords(&["color", "highlight"]),
    Entry::new(
        "appearance.system_accent",
        P,
        keys::SETTINGS_THEME_SYSTEM_ACCENT_COLOR,
    )
    .keywords(&["os color"]),
    Entry::new("appearance.high_contrast", P, keys::Q_MISC_HIGH_CONTRAST)
        .keywords(&["accessibility", "legible"]),
    Entry::new("appearance.custom_themes", P, keys::Q_MISC_CUSTOM_THEMES).keywords(&[
        "tdesktop-theme",
        "palette",
        "export",
    ]),
    Entry::new("appearance.import_theme", P, keys::Q_MISC_IMPORT_THEME_FILE)
        .keywords(&["load theme"]),
    Entry::new("appearance.create_theme", P, keys::SETTINGS_BG_THEME_CREATE)
        .keywords(&["editor", "new theme"]),
    Entry::new("appearance.wallpaper", P, keys::Q_PREFS_CHAT_WALLPAPER)
        .section(keys::Q_MISC_CHAT_BACKGROUND_2)
        .keywords(&["background", "solid color"]),
    Entry::new(
        "appearance.telegram_wallpapers",
        P,
        keys::Q_PREFS_TELEGRAM_WALLPAPERS,
    )
    .section(keys::Q_MISC_CHAT_BACKGROUND_2)
    .keywords(&["background", "gradient", "pattern"]),
    Entry::new(
        "appearance.tile_background",
        P,
        keys::Q_PREFS_TILE_BACKGROUND,
    )
    .section(keys::Q_MISC_CHAT_BACKGROUND_2)
    .keywords(&["repeat", "pattern"]),
    Entry::new("appearance.adaptive_wide", P, keys::SETTINGS_ADAPTIVE_WIDE).keywords(&[
        "layout",
        "wide screen",
        "centered chat",
    ]),
    Entry::new("appearance.text_size", P, keys::Q_PREFS_MESSAGE_TEXT_SIZE).keywords(&[
        "font size",
        "bigger",
        "smaller",
    ]),
    Entry::new("appearance.font_family", P, keys::Q_MISC_FONT_FAMILY)
        .keywords(&["typeface", "font"]),
    Entry::new(
        "appearance.interface_scale",
        P,
        keys::Q_PREFS_INTERFACE_SCALE,
    )
    .keywords(&["zoom", "dpi", "size"]),
    Entry::new("appearance.chat_style", P, keys::Q_PREFS_CHAT_STYLE)
        .keywords(&["bubbles", "plain", "layout"]),
    Entry::new("appearance.chat_list_rows", P, keys::Q_PREFS_CHAT_LIST_ROWS).keywords(&[
        "two lines",
        "three lines",
        "density",
        "preview",
    ]),
    Entry::new("appearance.media_icons", P, keys::Q_PREFS_MEDIA_ICONS)
        .section(keys::Q_PREFS_CHAT_LIST_ROWS)
        .keywords(&["photo icon"]),
    Entry::new(
        "appearance.rich_preview",
        P,
        keys::Q_PREFS_FORMATTED_PREVIEW_TEXT,
    )
    .section(keys::Q_PREFS_CHAT_LIST_ROWS)
    .keywords(&["bold", "italic", "markdown"]),
];
