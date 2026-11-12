//! Appearance preferences: theme, night mode, font size and interface scale.
use super::*;

/// Settings → Appearance slice: client-side look-and-feel, persisted as
/// JSON next to the account root (`appearance_prefs.json`). Entirely
/// client-side — no TDLib setting exists for the app theme, accent,
/// font size, or chat-list style (TDLib's `accentColor` is per-peer
/// name/profile tinting, not app chrome; chat backgrounds exist
/// server-side as `setChatBackground`, schema 1.8.67 :13473, but this
/// slice paints local solid colors only — see DECISIONS.md).
///
/// TGX reference (`Settings.java`, `SettingsThemeController.java`):
/// night modes None/Auto(lux)/Scheduled/System, per-theme accent color
/// ids, solid wallpapers via the backgrounds API, a text-size slider,
/// and bubble chat style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
    /// stories-high-contrast: maximum-contrast palette (pure-black
    /// surfaces, white text/borders). Wins over auto-night — an explicit
    /// accessibility choice is never silently reverted by the schedule.
    #[serde(rename = "high_contrast")]
    HighContrast,
}

/// Auto-night mode (TGX `NIGHT_MODE_*`, `Settings.java:460-464`). `Auto`
/// (ambient-light sensor) has no desktop equivalent, so Quill offers
/// Off / System (OS appearance) / Scheduled (local-time window).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoNight {
    #[default]
    Off,
    System,
    Scheduled,
}

/// Message text size bounds, in px. The default 14px matches the
/// previous hardcoded `text_sm()` at the 16px rem base.
pub const FONT_SIZE_MIN: u8 = 12;
pub const FONT_SIZE_MAX: u8 = 20;
pub const FONT_SIZE_DEFAULT: u8 = 14;

/// Clamp a font size into the supported range (the stepper can't
/// produce out-of-range values, but prefs files are user-editable).
pub fn clamp_font_size(px: u8) -> u8 {
    px.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX)
}

/// tdesktop's interface scale range (`style::kScaleMin` / `kScaleMax`), in
/// percent. 100 is the unscaled interface.
pub const INTERFACE_SCALE_MIN: u16 = 100;
pub const INTERFACE_SCALE_MAX: u16 = 300;
pub const INTERFACE_SCALE_DEFAULT: u16 = 100;
/// The scales the Appearance dialog offers.
pub const INTERFACE_SCALE_CHOICES: [u16; 7] = [100, 125, 150, 175, 200, 250, 300];

/// Clamp a stored scale into tdesktop's range and snap it to steps of 5
/// (prefs files are user-editable).
pub fn clamp_interface_scale(pct: u16) -> u16 {
    let clamped = pct.clamp(INTERFACE_SCALE_MIN, INTERFACE_SCALE_MAX);
    ((clamped + 2) / 5 * 5).clamp(INTERFACE_SCALE_MIN, INTERFACE_SCALE_MAX)
}

/// Where the chat wallpaper comes from: one choice, not a colour beside a
/// "use Telegram's" flag. The prefs file keeps its two keys
/// (`wallpaper_rgb`, `telegram_wallpaper`); [`WallpaperFile`] maps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "WallpaperFile", into = "WallpaperFile")]
pub enum WallpaperSource {
    /// The app theme's own background.
    #[default]
    Theme,
    /// A preset colour, 0xRRGGBB.
    Color(u32),
    /// The account's Telegram wallpaper (`updateDefaultBackground`) behind
    /// the messages.
    Telegram,
}

impl WallpaperSource {
    /// The preset colour, when that is the choice.
    pub fn color(self) -> Option<u32> {
        match self {
            Self::Color(rgb) => Some(rgb),
            Self::Theme | Self::Telegram => None,
        }
    }

    pub fn is_telegram(self) -> bool {
        self == Self::Telegram
    }
}

/// The two keys of `appearance_prefs.json` a [`WallpaperSource`] is stored
/// as. Telegram's wallpaper wins when both are set, as it always did.
#[derive(Serialize, Deserialize)]
struct WallpaperFile {
    #[serde(default)]
    wallpaper_rgb: Option<u32>,
    #[serde(default)]
    telegram_wallpaper: bool,
}

impl From<WallpaperFile> for WallpaperSource {
    fn from(file: WallpaperFile) -> Self {
        match (file.telegram_wallpaper, file.wallpaper_rgb) {
            (true, _) => Self::Telegram,
            (false, Some(rgb)) => Self::Color(rgb),
            (false, None) => Self::Theme,
        }
    }
}

impl From<WallpaperSource> for WallpaperFile {
    fn from(source: WallpaperSource) -> Self {
        Self {
            wallpaper_rgb: source.color(),
            telegram_wallpaper: source.is_telegram(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearancePrefs {
    /// Manual theme choice; auto-night overrides it while active.
    #[serde(default)]
    pub theme: ThemeChoice,
    #[serde(default)]
    pub auto_night: AutoNight,
    /// Scheduled auto-night window, minutes since local midnight
    /// (defaults 22:00 → 07:00, TGX's usual night window).
    #[serde(default = "default_night_start")]
    pub night_start_minutes: u16,
    #[serde(default = "default_night_end")]
    pub night_end_minutes: u16,
    /// Accent color of Quill Light and Quill Dark as 0xRRGGBB; 0 = their
    /// default accent. tdesktop's themes use `theme_accents`.
    #[serde(default)]
    pub accent_rgb: u32,
    /// tdesktop's day and night themes: the theme used while night mode
    /// is off, and the one used while it is on (manually or by
    /// auto-night).
    #[serde(default = "crate::app_theme::ThemeId::default_day")]
    pub day_theme: crate::app_theme::ThemeId,
    #[serde(default = "crate::app_theme::ThemeId::default_night")]
    pub night_theme: crate::app_theme::ThemeId,
    /// tdesktop `themesAccentColors`: the accent chosen for each built-in
    /// theme (by its id), 0xRRGGBB.
    #[serde(default)]
    pub theme_accents: std::collections::BTreeMap<String, u32>,
    /// tdesktop `systemAccentColorEnabled`: use the operating system's
    /// accent color instead of `accent_rgb` while the OS reports one.
    #[serde(default)]
    pub system_accent: bool,
    /// tdesktop `customFontFamily`: the interface font; empty = the
    /// platform default.
    #[serde(default)]
    pub font_family: String,
    /// `power_saving` flags that are on (animations kept still); 0 = all
    /// animations play, tdesktop's default.
    #[serde(default)]
    pub power_saving: u32,
    /// tdesktop `ignoreBatterySaving`: keep the animation switches as set
    /// even while the system battery saver is on.
    #[serde(default)]
    pub ignore_battery_saving: bool,
    /// Chat wallpaper: the theme background, a preset colour or the
    /// account's Telegram wallpaper.
    #[serde(flatten, default)]
    pub wallpaper: WallpaperSource,
    /// tdesktop "Tile background" for photo wallpapers, kept per theme
    /// (`tileDay` / `tileNight`): the photo repeats at its own size
    /// instead of covering the area.
    #[serde(default)]
    pub wallpaper_tile_day: bool,
    #[serde(default)]
    pub wallpaper_tile_night: bool,
    /// tdesktop `adaptiveForWide` ("Adaptive layout for wide screens"):
    /// in a chat at least `chat_wide::ADAPTIVE_CHAT_WIDE_WIDTH` wide, every
    /// message sits on the left in one column. tdesktop turns it on by
    /// default; Quill keeps it off so its default look is unchanged.
    #[serde(default)]
    pub adaptive_for_wide: bool,
    /// On Omarchy, take Omarchy's current theme and follow its switches.
    /// Choosing another theme turns this off.
    #[serde(default = "default_true")]
    pub follow_omarchy: bool,
    /// Interface scale in percent (tdesktop 100-300%). Scales the kit's
    /// rem size, so text and rem-based spacing grow together; widths drawn
    /// in fixed pixels do not.
    #[serde(default = "default_interface_scale")]
    pub interface_scale_pct: u16,
    /// Message text size in px.
    #[serde(default = "default_font_size")]
    pub font_size_px: u8,
    /// Bubble style (true) vs plain full-width rows (false).
    #[serde(default = "default_true")]
    pub bubbles: bool,
    /// Slice chatlist-list-style: chat-list preview lines — 2 (title +
    /// preview) or 3 (title + sender line + preview line).
    #[serde(default = "default_preview_lines")]
    pub preview_lines: u8,
    /// Slice chatlist-list-style: show a media-type icon before the
    /// chat-list preview text.
    #[serde(default)]
    pub chat_list_media_icons: bool,
    /// Slice chatlist-list-style: render formatted text (bold/italic/…)
    /// in the chat-list preview instead of plain text.
    #[serde(default)]
    pub chat_list_rich_preview: bool,
    /// tdesktop `archiveCollapsed`: the "Archived chats" row shrinks to a
    /// slim bar at the top of the chat list.
    #[serde(default)]
    pub archive_collapsed: bool,
    /// tdesktop `archiveInMainMenu`: the archive leaves the chat list and
    /// lives in the main menu.
    #[serde(default)]
    pub archive_in_main_menu: bool,
    /// tdesktop `quickDialogAction` ("Chat list quick action"): what a
    /// horizontal trackpad swipe on a chat row does. Disabled by default,
    /// as in tdesktop.
    #[serde(default)]
    pub swipe_action: crate::chat_swipe::SwipeAction,
    /// tdesktop `chatFiltersHorizontal` (inverted): "Tabs on the left".
    #[serde(default)]
    pub folder_tabs_view: crate::folder_icons::FolderTabsView,
    /// tdesktop `chatFiltersTabsMode`: text, icons, or both on the tabs.
    #[serde(default)]
    pub folder_tabs_mode: crate::folder_icons::FolderTabsMode,
    /// tdesktop `startMinimized`: open hidden in the tray.
    #[serde(default)]
    pub start_in_tray: bool,
    /// tdesktop `CloseBehavior::RunInBackground`: closing the window keeps
    /// Quill running in the tray. (Stored as `minimize_to_tray` since it
    /// first only covered the macOS minimize button.)
    #[serde(default)]
    pub minimize_to_tray: bool,
    /// tdesktop `WorkMode`: "Show tray icon" and "Show taskbar icon" (off
    /// keeps the main window out of the taskbar on Windows and X11; the tray
    /// reopens it). Saved as the two `show_*_icon` keys.
    #[serde(flatten)]
    pub work_mode: crate::tray::work_mode::WorkMode,
    /// tdesktop `trayIconMonochrome` ("Use monochrome icon", on by default):
    /// the Windows tray shows a one-color glyph that matches the taskbar.
    #[serde(default = "default_true")]
    pub tray_icon_monochrome: bool,
    /// tdesktop `nativeWindowFrame` ("Use system window frame", Linux and
    /// Windows): the operating system draws the title bar. Off by default.
    #[serde(default)]
    pub native_window_frame: bool,
}

fn default_night_start() -> u16 {
    22 * 60
}

fn default_night_end() -> u16 {
    7 * 60
}

fn default_interface_scale() -> u16 {
    INTERFACE_SCALE_DEFAULT
}

fn default_font_size() -> u8 {
    FONT_SIZE_DEFAULT
}

fn default_preview_lines() -> u8 {
    crate::chatlist_style::PREVIEW_LINES_DEFAULT
}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Light,
            auto_night: AutoNight::Off,
            night_start_minutes: default_night_start(),
            night_end_minutes: default_night_end(),
            accent_rgb: 0,
            day_theme: crate::app_theme::ThemeId::default_day(),
            night_theme: crate::app_theme::ThemeId::default_night(),
            theme_accents: Default::default(),
            system_accent: false,
            font_family: String::new(),
            power_saving: 0,
            ignore_battery_saving: false,
            wallpaper: WallpaperSource::Theme,
            wallpaper_tile_day: false,
            wallpaper_tile_night: false,
            adaptive_for_wide: false,
            follow_omarchy: true,
            interface_scale_pct: INTERFACE_SCALE_DEFAULT,
            font_size_px: FONT_SIZE_DEFAULT,
            bubbles: true,
            preview_lines: crate::chatlist_style::PREVIEW_LINES_DEFAULT,
            chat_list_media_icons: false,
            chat_list_rich_preview: false,
            archive_collapsed: false,
            archive_in_main_menu: false,
            swipe_action: crate::chat_swipe::SwipeAction::Disabled,
            folder_tabs_view: crate::folder_icons::FolderTabsView::Top,
            folder_tabs_mode: crate::folder_icons::FolderTabsMode::Default,
            start_in_tray: false,
            minimize_to_tray: false,
            work_mode: crate::tray::work_mode::WorkMode::WindowAndTray,
            tray_icon_monochrome: true,
            native_window_frame: false,
        }
    }
}

/// Load appearance prefs; missing or corrupt files fall back to
/// defaults (never a hard error — prefs must not block startup).
/// Out-of-range values from hand-edited JSON are sanitized here, at
/// the single load path: the font size clamps to 12–20 px and night
/// times wrap into 0..1440.
impl AppearancePrefs {
    /// The theme for night mode on or off.
    pub fn theme_for(&self, night: bool) -> &crate::app_theme::ThemeId {
        if night {
            &self.night_theme
        } else {
            &self.day_theme
        }
    }

    /// The accent chosen for a built-in theme (0xRRGGBB): Quill's themes
    /// keep the single `accent_rgb` they always had; tdesktop's each have
    /// their own entry.
    pub fn accent_for(&self, theme: crate::app_theme::BuiltinTheme) -> Option<u32> {
        if theme.is_quill() {
            return (self.accent_rgb != 0).then_some(self.accent_rgb);
        }
        self.theme_accents.get(theme.id()).copied()
    }

    /// tdesktop `SetupDefaultThemes` `apply`: `id` becomes the day or night
    /// theme and night mode follows, in one write. Omarchy following ends,
    /// and auto-night goes off when it would switch straight back
    /// (`auto_off`, tdesktop's "Disable" answer to its warning).
    pub fn choose_theme(&mut self, id: crate::app_theme::ThemeId, night: bool, auto_off: bool) {
        self.follow_omarchy = false;
        if night {
            self.night_theme = id;
            self.theme = ThemeChoice::Dark;
        } else {
            self.day_theme = id;
            self.theme = ThemeChoice::Light;
        }
        if auto_off {
            self.auto_night = AutoNight::Off;
        }
    }

    /// Whether photo wallpapers tile in the light or dark theme.
    pub fn wallpaper_tile(&self, dark: bool) -> bool {
        if dark {
            self.wallpaper_tile_night
        } else {
            self.wallpaper_tile_day
        }
    }

    pub fn set_wallpaper_tile(&mut self, dark: bool, tile: bool) {
        if dark {
            self.wallpaper_tile_night = tile;
        } else {
            self.wallpaper_tile_day = tile;
        }
    }
}

pub fn load_appearance_prefs(paths: &AccountPaths) -> AppearancePrefs {
    let mut prefs: AppearancePrefs = load_json_prefs(paths, "appearance_prefs.json");
    prefs.font_size_px = clamp_font_size(prefs.font_size_px);
    prefs.interface_scale_pct = clamp_interface_scale(prefs.interface_scale_pct);
    prefs.font_family = crate::font_choice::clean_family(&prefs.font_family);
    prefs.power_saving = crate::power_saving::sanitize(prefs.power_saving);
    prefs.preview_lines = crate::chatlist_style::clamp_preview_lines(prefs.preview_lines);
    prefs.night_start_minutes %= 24 * 60;
    prefs.night_end_minutes %= 24 * 60;
    prefs
}

/// Persist appearance prefs.
pub fn save_appearance_prefs(paths: &AccountPaths, prefs: &AppearancePrefs) {
    save_json_prefs(paths, "appearance_prefs.json", prefs)
}

/// Pure predicate behind scheduled auto-night: is `now` (minutes since
/// local midnight) inside the [start, end) night window? Windows that
/// wrap past midnight (22:00 → 07:00) are the normal case; start == end
/// is the empty window (never night).
pub fn night_active(start_minutes: u16, end_minutes: u16, now_minutes: u16) -> bool {
    if start_minutes <= end_minutes {
        now_minutes >= start_minutes && now_minutes < end_minutes
    } else {
        now_minutes >= start_minutes || now_minutes < end_minutes
    }
}

/// Minutes since local midnight, for scheduled auto-night (the
/// testable predicate is `night_active`). Uses the cross-platform
/// `local_time` (Unix `localtime_r`, Windows time-zone API).
pub fn local_minutes_since_midnight() -> u16 {
    let now = crate::local_time::now_unix();
    let local = crate::local_time::civil_local(now);
    u16::from(local.hour) * 60 + u16::from(local.minute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn interface_scale_clamps_and_snaps() {
        assert_eq!(clamp_interface_scale(0), 100);
        assert_eq!(clamp_interface_scale(100), 100);
        assert_eq!(clamp_interface_scale(133), 135);
        assert_eq!(clamp_interface_scale(150), 150);
        assert_eq!(clamp_interface_scale(999), 300);
        assert!(
            INTERFACE_SCALE_CHOICES
                .iter()
                .all(|c| clamp_interface_scale(*c) == *c)
        );
    }

    /// Settings → Appearance: prefs survive a save/load roundtrip and a
    /// missing file falls back to defaults (never an error).
    #[test]
    fn appearance_prefs_roundtrip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("quill-appearance-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());

        // Missing file → defaults (light theme, auto-night off, 14px, bubbles).
        let defaults = load_appearance_prefs(&paths);
        assert_eq!(defaults, AppearancePrefs::default());
        assert_eq!(defaults.theme, ThemeChoice::Light);
        assert_eq!(defaults.auto_night, AutoNight::Off);
        assert_eq!(defaults.font_size_px, FONT_SIZE_DEFAULT);
        assert!(defaults.bubbles);

        let prefs = AppearancePrefs {
            theme: ThemeChoice::Dark,
            auto_night: AutoNight::Scheduled,
            night_start_minutes: 23 * 60,
            night_end_minutes: 6 * 60,
            accent_rgb: 0x2f81f7,
            day_theme: crate::app_theme::ThemeId::Builtin(crate::app_theme::BuiltinTheme::Day),
            night_theme: crate::app_theme::ThemeId::Custom("Ocean".into()),
            theme_accents: [("night".to_string(), 0xca7896)].into(),
            system_accent: true,
            font_family: "Avenir".into(),
            power_saving: crate::power_saving::Flag::StickersChat.bit(),
            ignore_battery_saving: true,
            wallpaper: WallpaperSource::Telegram,
            wallpaper_tile_day: true,
            wallpaper_tile_night: false,
            adaptive_for_wide: true,
            follow_omarchy: false,
            interface_scale_pct: 150,
            font_size_px: 17,
            bubbles: false,
            preview_lines: 3,
            chat_list_media_icons: true,
            chat_list_rich_preview: true,
            archive_collapsed: true,
            archive_in_main_menu: true,
            swipe_action: crate::chat_swipe::SwipeAction::Archive,
            folder_tabs_view: crate::folder_icons::FolderTabsView::Left,
            folder_tabs_mode: crate::folder_icons::FolderTabsMode::IconsOnly,
            start_in_tray: true,
            minimize_to_tray: true,
            work_mode: crate::tray::work_mode::WorkMode::TrayOnly,
            tray_icon_monochrome: false,
            native_window_frame: true,
        };
        save_appearance_prefs(&paths, &prefs);
        assert_eq!(load_appearance_prefs(&paths), prefs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Prefs written before "Use monochrome icon" existed keep it on, as
    /// tdesktop's default, and an explicit off survives.
    #[test]
    fn monochrome_tray_icon_defaults_on() {
        let old: AppearancePrefs = serde_json::from_str("{}").unwrap();
        assert!(old.tray_icon_monochrome);
        let off: AppearancePrefs =
            serde_json::from_str(r#"{"tray_icon_monochrome":false}"#).unwrap();
        assert!(!off.tray_icon_monochrome);
    }

    /// Settings → Appearance: a corrupt prefs file falls back to
    /// defaults instead of blocking startup.
    #[test]
    fn appearance_prefs_corrupt_file_falls_back() {
        let dir =
            std::env::temp_dir().join(format!("quill-appearance-corrupt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        fs::create_dir_all(&paths.root).unwrap();
        fs::write(
            paths.root.join("appearance_prefs.json"),
            b"{ this is not json",
        )
        .unwrap();
        assert_eq!(load_appearance_prefs(&paths), AppearancePrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Settings → Appearance: scheduled auto-night boundaries, including
    /// the wrap-past-midnight case and the empty window.
    #[test]
    fn night_active_cases() {
        // Wrap-past-midnight window 22:00 → 07:00.
        assert!(night_active(22 * 60, 7 * 60, 23 * 60));
        assert!(night_active(22 * 60, 7 * 60, 3 * 60));
        assert!(!night_active(22 * 60, 7 * 60, 12 * 60));
        // Boundaries: inclusive start, exclusive end.
        assert!(night_active(22 * 60, 7 * 60, 22 * 60));
        assert!(!night_active(22 * 60, 7 * 60, 7 * 60));
        // Same-day window 09:00 → 17:00.
        assert!(night_active(9 * 60, 17 * 60, 12 * 60));
        assert!(!night_active(9 * 60, 17 * 60, 8 * 60));
        assert!(!night_active(9 * 60, 17 * 60, 17 * 60));
        // Degenerate window (start == end) is never night.
        assert!(!night_active(7 * 60, 7 * 60, 7 * 60));
        assert!(!night_active(7 * 60, 7 * 60, 12 * 60));
    }

    /// Settings → Appearance: out-of-range font sizes clamp to 12–20px.
    #[test]
    fn font_size_clamps_to_range() {
        assert_eq!(clamp_font_size(0), FONT_SIZE_MIN);
        assert_eq!(clamp_font_size(11), FONT_SIZE_MIN);
        assert_eq!(clamp_font_size(14), 14);
        assert_eq!(clamp_font_size(20), 20);
        assert_eq!(clamp_font_size(255), FONT_SIZE_MAX);
    }

    /// Settings → Appearance: the load path sanitizes hand-edited
    /// `appearance_prefs.json` — this validates that
    /// `load_appearance_prefs` clamps an out-of-range `font_size_px`
    /// to 12–20 and wraps night times into 0..1440, while in-range
    /// values pass through untouched (roundtrip, corrupt-fallback,
    /// night-case and pure-clamp behavior are covered by the tests
    /// above).
    #[test]
    fn appearance_prefs_load_sanitizes_out_of_range() {
        let dir =
            std::env::temp_dir().join(format!("quill-appearance-sanitize-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        fs::create_dir_all(&paths.root).unwrap();

        // Out-of-range values: font clamps, night times wrap.
        fs::write(
            paths.root.join("appearance_prefs.json"),
            br#"{"font_size_px": 255, "night_start_minutes": 5000, "night_end_minutes": 1500}"#,
        )
        .unwrap();
        let prefs = load_appearance_prefs(&paths);
        assert_eq!(prefs.font_size_px, FONT_SIZE_MAX);
        assert_eq!(prefs.night_start_minutes, 5000 % (24 * 60));
        assert_eq!(prefs.night_end_minutes, 1500 % (24 * 60));

        // In-range values are not touched.
        fs::write(
            paths.root.join("appearance_prefs.json"),
            br#"{"font_size_px": 16, "night_start_minutes": 1380, "night_end_minutes": 420}"#,
        )
        .unwrap();
        let prefs = load_appearance_prefs(&paths);
        assert_eq!(prefs.font_size_px, 16);
        assert_eq!(prefs.night_start_minutes, 1380);
        assert_eq!(prefs.night_end_minutes, 420);

        // Power saving keeps only the known bits; the font loses control
        // characters.
        fs::write(
            paths.root.join("appearance_prefs.json"),
            br#"{"power_saving": 4294967295, "font_family": " Fira\nCode "}"#,
        )
        .unwrap();
        let prefs = load_appearance_prefs(&paths);
        assert_eq!(prefs.power_saving, crate::power_saving::ALL_BITS);
        assert_eq!(prefs.font_family, "FiraCode");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod theme_choice_tests {
    use super::*;

    /// stories-high-contrast: the new variant serializes as
    /// "high_contrast" and round-trips; prefs saved before the variant
    /// existed still load (missing theme → default Light).
    #[test]
    fn high_contrast_theme_choice_serde() {
        let json = serde_json::to_string(&ThemeChoice::HighContrast).unwrap();
        assert_eq!(json, "\"high_contrast\"");
        assert_eq!(
            serde_json::from_str::<ThemeChoice>("\"high_contrast\"").unwrap(),
            ThemeChoice::HighContrast
        );
        assert_eq!(
            serde_json::from_str::<ThemeChoice>("\"dark\"").unwrap(),
            ThemeChoice::Dark
        );
        let legacy: AppearancePrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.theme, ThemeChoice::Light);
    }

    /// Prefs saved before themes keep loading: Classic by day, Tinted at
    /// night, and an old single accent still colours built-in themes until
    /// one gets its own.
    #[test]
    fn theme_slots_and_accents() {
        use crate::app_theme::{BuiltinTheme, ThemeId};
        let old: AppearancePrefs =
            serde_json::from_str(r#"{"theme":"dark","accent_rgb":3108855}"#).unwrap();
        assert_eq!(
            old.theme_for(false),
            &ThemeId::Builtin(BuiltinTheme::QuillLight)
        );
        assert_eq!(
            old.theme_for(true),
            &ThemeId::Builtin(BuiltinTheme::QuillDark)
        );
        // The old accent stays on Quill's themes only.
        assert_eq!(old.accent_for(BuiltinTheme::QuillDark), Some(0x2f6ff7));
        assert_eq!(old.accent_for(BuiltinTheme::Night), None);
        let mut prefs = old;
        prefs.theme_accents.insert("night".into(), 0xca7896);
        assert_eq!(prefs.accent_for(BuiltinTheme::Night), Some(0xca7896));
        assert_eq!(
            AppearancePrefs::default().accent_for(BuiltinTheme::Day),
            None
        );
        // Choosing a theme writes its slot, the manual night choice and
        // the Omarchy / auto-night switches together.
        let mut chosen = AppearancePrefs {
            auto_night: AutoNight::System,
            ..AppearancePrefs::default()
        };
        chosen.choose_theme(ThemeId::Builtin(BuiltinTheme::Night), true, true);
        assert!(!chosen.follow_omarchy);
        assert_eq!(chosen.theme, ThemeChoice::Dark);
        assert_eq!(chosen.auto_night, AutoNight::Off);
        assert_eq!(
            chosen.theme_for(true),
            &ThemeId::Builtin(BuiltinTheme::Night)
        );
        chosen.auto_night = AutoNight::Scheduled;
        chosen.choose_theme(ThemeId::Builtin(BuiltinTheme::Day), false, false);
        assert_eq!(chosen.theme, ThemeChoice::Light);
        assert_eq!(chosen.auto_night, AutoNight::Scheduled);
        // A custom theme slot survives a round trip.
        prefs.night_theme = ThemeId::Custom("Ocean".into());
        let json = serde_json::to_string(&prefs).unwrap();
        let back: AppearancePrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back.night_theme, ThemeId::Custom("Ocean".into()));
    }

    #[test]
    fn wallpaper_and_work_mode_keep_their_file_keys() {
        use crate::tray::work_mode::WorkMode;
        let old: AppearancePrefs = serde_json::from_str(
            r#"{"wallpaper_rgb":14,"telegram_wallpaper":true,"show_tray_icon":false,"show_taskbar_icon":false}"#,
        )
        .unwrap();
        // Telegram's wallpaper wins over the colour; both switches off
        // reads as window only.
        assert_eq!(old.wallpaper, WallpaperSource::Telegram);
        assert_eq!(old.work_mode, WorkMode::WindowOnly);
        let color: AppearancePrefs =
            serde_json::from_str(r#"{"wallpaper_rgb":14,"show_taskbar_icon":false}"#).unwrap();
        assert_eq!(color.wallpaper, WallpaperSource::Color(14));
        assert_eq!(color.work_mode, WorkMode::TrayOnly);
        let json = serde_json::to_value(&color).unwrap();
        assert_eq!(json["wallpaper_rgb"], 14);
        assert_eq!(json["telegram_wallpaper"], false);
        assert_eq!(json["show_tray_icon"], true);
        assert_eq!(json["show_taskbar_icon"], false);
        let bare: AppearancePrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(bare.wallpaper, WallpaperSource::Theme);
        assert_eq!(bare.work_mode, WorkMode::WindowAndTray);
    }
}
