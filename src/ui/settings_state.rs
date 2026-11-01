//! Settings state: the settings window, appearance, shortcuts, storage, proxy, translation and app updates.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::*;
use quill::data_settings::AutoDownloadNetSettings;
use quill::data_settings::NetworkKind;

pub(crate) struct SettingsUi {
    /// The settings window covers the chat list and the chat.
    pub(super) open: bool,
    /// The page on screen. `None`: none chosen yet, so a wide window shows
    /// Account and a narrow one the page list.
    pub(super) page: Option<quill::settings_catalog::Page>,
    /// A page reached from a row of `page`, with a back arrow.
    pub(super) sub: Option<super::settings_window::SubPage>,
    /// The Log out confirmation is on screen.
    pub(super) log_out_confirm: bool,
    /// The search field and its results.
    pub(super) search: super::settings_window::SettingsSearch,
    /// Holds focus inside the settings window, so Esc steps back.
    pub(super) focus: FocusHandle,
    /// Parity slice (platform-custom-keybindings): the rebindable action id
    /// currently capturing a keystroke, if any.
    pub(super) keybinding_capture: Option<String>,
    /// Parity slice (platform-custom-keybindings): a capture that was
    /// refused (fixed chrome or another rebindable action). The shortcuts
    /// chip keeps showing the chord that is actually bound.
    pub(super) keybinding_error: Option<(String, String)>,
    /// Parity slice (platform-custom-keybindings): focus handle for the
    /// keystroke-capture row.
    pub(super) keybinding_focus: FocusHandle,
    /// Parity slice (platform-custom-keybindings): saved shortcut overrides
    /// applied to the keymap once the live driver is ready.
    pub(super) keybindings_applied: bool,
    pub(super) sticker_settings_open: bool,
    pub(super) storage_scroll: super::dialogs::data_storage::StorageScroll,
    /// Settings → Appearance slice: last `(theme mode, accent)` pushed
    /// into the global component theme, so `apply_appearance` only
    /// notifies (re-renders) when something actually changed.
    /// The middle value is the applied palette's fingerprint (the accent
    /// under high contrast).
    pub(super) appearance_applied: Option<(ThemeMode, u64, bool, u16, String)>,
    /// App themes: custom theme list, the applied palette, the editor.
    pub(super) app_theme: super::app_theme::AppThemeUi,
    /// The operating system's accent color (0xRRGGBB), when it reports one
    /// and has been read (`refresh_system_accent`).
    pub(super) system_accent: Option<u32>,
    /// `system_accent` was read at least once.
    pub(super) system_accent_probed: bool,
    /// Settings → Appearance: the searchable font family list.
    pub(super) font_picker: Entity<
        gpui_kit::component::select::SelectState<
            gpui_kit::component::select::SearchableVec<SharedString>,
        >,
    >,
    /// Settings → Appearance: the custom accent color field (kit
    /// `ColorSelect`), tdesktop's "custom" accent circle. Holds the last
    /// custom color; choosing one sets `appearance.accent_rgb`.
    pub(super) accent_picker: Entity<gpui_kit::component::color_picker::ColorPickerState>,
    /// Slice S4: the Data & Storage per-network editor — the network
    /// being edited plus its draft settings (saved or discarded
    /// explicitly, never applied optimistically).
    pub(super) data_storage_editor: Option<(NetworkKind, AutoDownloadNetSettings)>,
    /// The step sliders of the network editor, the local storage limits and
    /// the auto-delete picker (`index_sliders.rs`).
    pub(super) index_sliders: super::index_sliders::IndexSliders,
    /// Slice S4: the "Clear cache" button is awaiting its second,
    /// confirming tap.
    pub(super) storage_confirm: Option<StorageClear>,
    /// Batch 6: the file types ticked for "Clear selected".
    pub(super) storage_selected: std::collections::BTreeSet<&'static str>,
    /// `parity:proxy-settings`: proxy list / editor / link-confirm state.
    pub(super) proxy: super::proxy::ProxyUi,
    /// Translation: prefs (`translate_prefs.json`), the translate dialog,
    /// the bar's toast.
    pub(super) translate: super::translate_ui::TranslateUi,
    pub(super) update_state: quill::updater::UpdateState,
    pub(super) update_banner_dismissed: bool,
    /// The What's New window and the release notes it shows.
    pub(super) whats_new: super::whats_new::WhatsNewUi,
    /// The "No" answer of the phone-number prompt was chosen: show the note.
    pub(super) phone_change_note: bool,
    /// Advanced and experimental settings, the secret codes, the test
    /// environment (`ui/advanced_state.rs`).
    pub(super) advanced: super::advanced_state::AdvancedUi,
    /// Settings → Chat → "Choose emoji set" and the set in use (`ui/emoji_style_ui.rs`).
    pub(super) emoji_style: super::emoji_style_ui::EmojiStyleUi,
}

impl SettingsUi {
    pub(super) fn new(
        cx: &mut Context<QuillApp>,
        font_picker: Entity<
            gpui_kit::component::select::SelectState<
                gpui_kit::component::select::SearchableVec<SharedString>,
            >,
        >,
        accent_picker: Entity<gpui_kit::component::color_picker::ColorPickerState>,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        Self {
            open: false,
            page: None,
            sub: None,
            log_out_confirm: false,
            search: Default::default(),
            focus: cx.focus_handle(),
            keybinding_capture: None,
            keybinding_error: None,
            keybinding_focus: cx.focus_handle(),
            keybindings_applied: false,
            sticker_settings_open: false,
            storage_scroll: super::dialogs::data_storage::StorageScroll::new(),
            appearance_applied: None,
            app_theme: Default::default(),
            system_accent: None,
            system_accent_probed: false,
            font_picker,
            accent_picker,
            data_storage_editor: None,
            index_sliders: Default::default(),
            storage_confirm: None,
            storage_selected: Default::default(),
            proxy: Default::default(),
            translate: super::translate_ui::TranslateUi::load(),
            update_state: if demo.is_none() {
                quill::update_install::startup_state()
            } else {
                quill::updater::UpdateState::Idle
            },
            update_banner_dismissed: false,
            whats_new: Default::default(),
            phone_change_note: false,
            advanced: super::advanced_state::AdvancedUi::new(cx, demo.is_none()),
            emoji_style: Default::default(),
        }
    }
}
