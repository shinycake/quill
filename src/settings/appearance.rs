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
    /// Accent color as 0xRRGGBB; 0 = the theme's default accent.
    #[serde(default)]
    pub accent_rgb: u32,
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
    /// Chat wallpaper as 0xRRGGBB; None = the theme background.
    #[serde(default)]
    pub wallpaper_rgb: Option<u32>,
    /// Show the account's Telegram wallpaper (`updateDefaultBackground`)
    /// behind the messages instead of the preset color.
    #[serde(default)]
    pub telegram_wallpaper: bool,
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
    #[serde(default)]
    pub start_in_tray: bool,
    /// tdesktop `CloseBehavior::RunInBackground`: closing the window keeps
    /// Quill running in the tray. (Stored as `minimize_to_tray` since it
    /// first only covered the macOS minimize button.)
    #[serde(default)]
    pub minimize_to_tray: bool,
    /// tdesktop `WorkMode` tray bit ("Show tray icon").
    #[serde(default = "default_true")]
    pub show_tray_icon: bool,
    /// tdesktop `macWarnBeforeQuit`: hold Cmd+Q to quit (macOS only).
    #[serde(default = "default_true")]
    pub mac_warn_before_quit: bool,
    #[serde(default = "default_true")]
    pub check_updates_on_launch: bool,
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
            system_accent: false,
            font_family: String::new(),
            power_saving: 0,
            wallpaper_rgb: None,
            telegram_wallpaper: false,
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
            show_tray_icon: true,
            mac_warn_before_quit: true,
            check_updates_on_launch: true,
        }
    }
}

/// Load appearance prefs; missing or corrupt files fall back to
/// defaults (never a hard error — prefs must not block startup).
/// Out-of-range values from hand-edited JSON are sanitized here, at
/// the single load path: the font size clamps to 12–20 px and night
/// times wrap into 0..1440.
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

/// Persist appearance prefs; failures are returned to the caller to
/// surface in the status note.
pub fn save_appearance_prefs(paths: &AccountPaths, prefs: &AppearancePrefs) -> std::io::Result<()> {
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
            system_accent: true,
            font_family: "Avenir".into(),
            power_saving: crate::power_saving::Flag::StickersChat.bit(),
            wallpaper_rgb: Some(0x0e1621),
            telegram_wallpaper: true,
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
            show_tray_icon: false,
            mac_warn_before_quit: false,
            check_updates_on_launch: false,
        };
        save_appearance_prefs(&paths, &prefs).unwrap();
        assert_eq!(load_appearance_prefs(&paths), prefs);
        let _ = std::fs::remove_dir_all(&dir);
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
}
