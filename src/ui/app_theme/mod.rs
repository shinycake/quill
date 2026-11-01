//! App themes in the UI (tdesktop `settings_chat.cpp` theme chooser,
//! `window_theme.cpp`, `window_theme_editor*.cpp`): which theme is shown,
//! the palette installed for Quill's tokens and the kit, the defaults the
//! theme gives chats (outgoing bubbles, the chat background), the chooser
//! in Settings → Appearance and the palette editor.
//!
//! The palette of the applied theme is installed process-wide (like the
//! light/dark flag in `chat_theme`); a chat's emoji theme still recolours
//! its rows on top of it (`chat_palette`).
#![deny(clippy::indexing_slicing)]

pub(crate) mod call_chrome;
mod chooser;
mod editor;
pub(crate) mod kit_colors;
mod omarchy;
#[cfg(feature = "demo-capture")]
mod omarchy_demo;
mod reads;

use super::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::app_theme::{
    BuiltinTheme, Palette, PaletteKey, ThemeBackground, ThemeFile, ThemeId, custom_palette, store,
};
use quill::lang::{keys, tr};
use quill::telegram::envelope::{BackgroundFill, ThemeSettings};
use std::path::PathBuf;

/// The installed palette Quill's tokens read. One per process; one per
/// thread in tests, like the light/dark flag (`chat_theme`).
#[cfg(not(test))]
mod installed_state {
    use quill::app_theme::PaletteKey;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static COLORS: [AtomicU32; PaletteKey::COUNT] =
        [const { AtomicU32::new(0) }; PaletteKey::COUNT];

    pub(super) fn get(key: PaletteKey) -> Option<u32> {
        if !ACTIVE.load(Ordering::Relaxed) {
            return None;
        }
        COLORS
            .get(key.index())
            .map(|color| color.load(Ordering::Relaxed))
    }

    pub(super) fn set(colors: Option<&[u32; PaletteKey::COUNT]>) {
        match colors {
            Some(colors) => {
                for (slot, value) in COLORS.iter().zip(colors) {
                    slot.store(*value, Ordering::Relaxed);
                }
                ACTIVE.store(true, Ordering::Relaxed);
            }
            None => ACTIVE.store(false, Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod installed_state {
    use quill::app_theme::PaletteKey;
    use std::cell::Cell;

    thread_local! {
        static COLORS: Cell<Option<[u32; PaletteKey::COUNT]>> = const { Cell::new(None) };
    }

    pub(super) fn get(key: PaletteKey) -> Option<u32> {
        COLORS.with(Cell::get).map(|c| c[key.index()])
    }

    pub(super) fn set(colors: Option<&[u32; PaletteKey::COUNT]>) {
        COLORS.with(|c| c.set(colors.copied()));
    }
}

/// Tests: install (or clear) a palette on this thread.
#[cfg(test)]
pub(crate) fn install_for_test(colors: Option<&[u32; PaletteKey::COUNT]>) {
    installed_state::set(colors);
}

/// The installed theme's colour for `key` (0xRRGGBBAA), or `None` before a
/// theme is applied and under high contrast.
#[inline]
pub(crate) fn installed(key: PaletteKey) -> Option<u32> {
    installed_state::get(key)
}

/// What the applied theme gives chats that have no theme of their own.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChatDefaults {
    /// Outgoing bubbles (`msgOutBg`) and the accent in them
    /// (`msgOutServiceFg`), as a chat theme would give them.
    pub settings: ThemeSettings,
    /// The outgoing fill is dark (white text on it).
    pub dark: bool,
    /// The theme's chat background.
    pub background: Option<ThemeBackground>,
}

impl ChatDefaults {
    fn from_palette(palette: &Palette) -> Self {
        let out = palette.rgb(PaletteKey::MsgOutBg);
        Self {
            settings: ThemeSettings {
                accent_color: palette.rgb(PaletteKey::WindowBgActive),
                background: None,
                outgoing_fill: Some(BackgroundFill::Solid(out)),
                outgoing_accent_color: Some(palette.rgb(PaletteKey::MsgOutServiceFg)),
            },
            dark: quill::theme_colorize::Hsl::of(out).lightness < 0.5,
            background: palette.background.clone(),
        }
    }
}

/// A custom theme in the chooser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CustomTheme {
    pub name: String,
    /// Background, outgoing and incoming bubble, for the preview tile.
    pub preview: [u32; 3],
    pub dark: bool,
}

impl CustomTheme {
    fn of(name: String, palette: &Palette) -> Self {
        Self {
            name,
            preview: preview_colors(palette),
            dark: palette.is_dark(),
        }
    }
}

/// A palette's preview tile colours: chat background, outgoing bubble,
/// incoming bubble.
pub(crate) fn preview_colors(palette: &Palette) -> [u32; 3] {
    [
        palette
            .background
            .as_ref()
            .map_or(palette.rgb(PaletteKey::WindowBg), ThemeBackground::average),
        palette.rgb(PaletteKey::MsgOutBg),
        palette.rgb(PaletteKey::MsgInBg),
    ]
}

/// Theme state of Settings → Appearance.
#[derive(Default)]
pub(crate) struct AppThemeUi {
    /// Custom themes in the themes folder, read when the chooser first
    /// shows and after every change.
    pub(super) custom: Option<Vec<CustomTheme>>,
    /// The read of the themes folder running for `custom`.
    custom_read: Option<Task<()>>,
    /// The last custom theme read: its name and palette (`None` when the
    /// file could not be read).
    custom_cache: Option<(String, Option<Palette>)>,
    /// The custom theme being read off the UI thread, by name.
    palette_read: Option<(String, Task<()>)>,
    /// The theme and palette applied last (outside high contrast).
    pub(super) applied: Option<(ThemeId, Palette)>,
    /// Defaults for chats without a theme; `None` under high contrast.
    pub(super) chat_defaults: Option<ChatDefaults>,
    /// The palette editor, while it is open.
    pub(super) editor: Option<editor::ThemeEditor>,
    /// Omarchy's current theme, once looked for (`omarchy.rs`).
    pub(super) omarchy: Option<omarchy::OmarchyState>,
    omarchy_probed: bool,
    omarchy_watched: bool,
}

/// The colour standing for a theme image: the 1x1 PNG's, else the mean of
/// a decoded image.
fn image_color(file: &ThemeFile) -> Option<u32> {
    let image = file.background.as_ref()?;
    if let Some(rgb) = quill::app_theme::png::solid_color(&image.bytes) {
        return Some(rgb);
    }
    let decoded = image::load_from_memory(&image.bytes).ok()?;
    let pixel = decoded
        .resize_exact(1, 1, image::imageops::FilterType::Triangle)
        .to_rgb8();
    let [r, g, b] = pixel.get_pixel(0, 0).0;
    Some((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
}

/// The palette of a custom theme file.
pub(crate) fn file_palette(file: &ThemeFile) -> Palette {
    custom_palette(file, image_color(file))
}

impl QuillApp {
    /// The folder custom themes are stored in.
    pub(crate) fn themes_dir() -> PathBuf {
        store::themes_dir(&Self::appearance_paths().root)
    }

    /// Whether night mode is on now: the manual choice, or auto-night
    /// (the OS appearance, or the scheduled window) while it is enabled.
    pub(crate) fn night_mode_now(&self, cx: &App) -> bool {
        use quill::settings::{AutoNight, ThemeChoice, local_minutes_since_midnight, night_active};
        match self.appearance.auto_night {
            AutoNight::Off => self.appearance.theme != ThemeChoice::Light,
            // On Linux without a desktop portal this reports Light; the mode
            // is still honest, it follows what the platform says.
            AutoNight::System => matches!(
                cx.window_appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
            AutoNight::Scheduled => night_active(
                self.appearance.night_start_minutes,
                self.appearance.night_end_minutes,
                local_minutes_since_midnight(),
            ),
        }
    }

    /// The accent for a built-in theme: the OS accent while "System accent
    /// color" is on and the OS reports one, else the theme's chosen one.
    pub(crate) fn theme_accent(&self, theme: BuiltinTheme) -> Option<u32> {
        if self.appearance.system_accent
            && let Some(system) = self.settings.system_accent
        {
            return Some(system);
        }
        self.appearance.accent_for(theme)
    }

    /// The theme to show and its palette: the editor's while it is open,
    /// else the day or night theme (a custom theme that cannot be read
    /// falls back to the built-in default for that mode).
    pub(crate) fn resolve_app_theme(
        &mut self,
        night: bool,
        cx: &mut Context<Self>,
    ) -> (ThemeId, Palette) {
        if let Some(editor) = &self.settings.app_theme.editor {
            return (editor.preview_id(), editor.palette.clone());
        }
        if self.appearance.follow_omarchy
            && let Some(palette) = self.omarchy_palette()
        {
            return (ThemeId::Omarchy, palette);
        }
        let id = self.appearance.theme_for(night).clone();
        match &id {
            ThemeId::Builtin(theme) => {
                let accent = self.theme_accent(*theme);
                (id.clone(), theme.palette_with_accent(accent))
            }
            ThemeId::Custom(_) | ThemeId::Omarchy => {
                if let ThemeId::Custom(name) = &id {
                    match self.custom_theme_palette(name, cx) {
                        reads::PaletteRead::Read(Some(palette)) => return (id, palette),
                        reads::PaletteRead::Read(None) => {}
                        reads::PaletteRead::Reading(shown) => return shown,
                    }
                }
                let fallback = if night {
                    ThemeId::default_night()
                } else {
                    ThemeId::default_day()
                };
                let theme = fallback.builtin().unwrap_or(BuiltinTheme::QuillLight);
                (
                    fallback,
                    theme.palette_with_accent(self.theme_accent(theme)),
                )
            }
        }
    }

    /// Record the applied theme (`None` under high contrast). When
    /// `painted`, its palette drives Quill's tokens and chats get its
    /// defaults; Quill Light and Quill Dark are not painted, so they look
    /// exactly as Quill did before themes (token pairs, kit colours, accent
    /// bubbles, no theme wallpaper).
    pub(crate) fn install_app_theme(&mut self, applied: Option<(ThemeId, Palette)>, painted: bool) {
        let painted = applied.as_ref().filter(|_| painted).map(|(_, p)| p);
        installed_state::set(painted.map(Palette::colors));
        self.settings.app_theme.chat_defaults = painted.map(ChatDefaults::from_palette);
        self.settings.app_theme.applied = applied;
    }

    /// The chat theme for chats that have none: the app theme's outgoing
    /// bubbles, and whether their fill is dark.
    pub(crate) fn default_chat_theme(&self) -> Option<(&ThemeSettings, bool)> {
        self.settings
            .app_theme
            .chat_defaults
            .as_ref()
            .map(|d| (&d.settings, d.dark))
    }

    /// The app theme's chat background, for chats without a wallpaper.
    pub(super) fn theme_wallpaper(&self) -> Option<super::wallpaper::Wallpaper> {
        use super::wallpaper::Wallpaper;
        let background = self
            .settings
            .app_theme
            .chat_defaults
            .as_ref()?
            .background
            .as_ref()?;
        Some(match background {
            ThemeBackground::Solid(rgb) => Wallpaper::Solid(*rgb),
            ThemeBackground::Gradient(colors) => {
                Wallpaper::Fill(BackgroundFill::Freeform(colors.clone()), 0)
            }
        })
    }

    /// The names in the themes folder, for picking a free one.
    pub(crate) fn custom_theme_names(&self) -> Vec<String> {
        store::list(&Self::themes_dir())
    }

    /// Forget what was read from the themes folder after a change there.
    fn refresh_custom_themes(&mut self) {
        let themes = &mut self.settings.app_theme;
        themes.custom = None;
        themes.custom_read = None;
        themes.custom_cache = None;
        themes.palette_read = None;
    }

    /// Show `id` (tdesktop `SetupDefaultThemes` `apply`): it becomes the
    /// day or night theme, by its colours, and night mode follows. When
    /// auto-night would switch straight back, it is turned off, as
    /// tdesktop's "Disable" answer to its auto-night warning does.
    pub(crate) fn choose_app_theme(&mut self, id: ThemeId, night: bool, cx: &mut Context<Self>) {
        use quill::settings::{AutoNight, ThemeChoice};
        let auto_disagrees =
            self.appearance.auto_night != AutoNight::Off && self.night_mode_now(cx) != night;
        if auto_disagrees {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MISC_AUTO_NIGHT_MODE_IS_OFF)));
        }
        self.set_appearance(cx, |a| {
            a.follow_omarchy = false;
            if night {
                a.night_theme = id;
                a.theme = ThemeChoice::Dark;
            } else {
                a.day_theme = id;
                a.theme = ThemeChoice::Light;
            }
            if auto_disagrees {
                a.auto_night = AutoNight::Off;
            }
        });
    }

    /// tdesktop's accent circles: `accent` (0xRRGGBB) for the built-in
    /// `theme`; turns "System accent color" off. Quill's two themes share
    /// the single accent they always had, and their own accent is stored
    /// as "default".
    pub(crate) fn set_theme_accent(
        &mut self,
        theme: BuiltinTheme,
        accent: u32,
        cx: &mut Context<Self>,
    ) {
        self.set_appearance(cx, |a| {
            if theme.is_quill() {
                a.accent_rgb = if accent == theme.accent() { 0 } else { accent };
            } else {
                a.theme_accents.insert(theme.id().to_string(), accent);
            }
            a.system_accent = false;
        });
    }

    /// The built-in theme on screen, if the applied one is built in.
    pub(crate) fn applied_builtin_theme(&self) -> Option<BuiltinTheme> {
        self.settings
            .app_theme
            .applied
            .as_ref()
            .and_then(|(id, _)| id.builtin())
    }
}

#[cfg(test)]
mod tests {
    use super::{ChatDefaults, installed, installed_state};
    use quill::app_theme::{BuiltinTheme, PaletteKey};

    #[test]
    fn the_installed_palette_is_scoped_to_the_thread_and_clears() {
        assert_eq!(installed(PaletteKey::WindowBg), None);
        let palette = BuiltinTheme::Night.palette();
        installed_state::set(Some(palette.colors()));
        assert_eq!(installed(PaletteKey::WindowBg), Some(0x282e33ff));
        installed_state::set(None);
        assert_eq!(installed(PaletteKey::WindowBg), None);
    }

    #[test]
    fn chat_defaults_come_from_the_outgoing_keys() {
        let classic = ChatDefaults::from_palette(&BuiltinTheme::Classic.palette());
        assert!(!classic.dark, "light green bubbles take black text");
        assert_eq!(classic.settings.outgoing_accent_color, Some(0x45a32d));
        let tinted = ChatDefaults::from_palette(&BuiltinTheme::Tinted.palette());
        assert!(tinted.dark);
        assert_eq!(tinted.settings.accent_color, 0x5288c1);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod ui_tests;
