//! Settings boxes: appearance, shortcuts, storage, proxy, translation and app updates.

use super::*;
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::*;
use quill::data_settings::AutoDownloadNetSettings;
use quill::data_settings::NetworkKind;

pub(crate) struct SettingsUi {
    pub(super) open: bool,
    pub(super) page: Option<&'static str>,
    /// Settings → Appearance slice: the dialog is on screen.
    pub(super) appearance_open: bool,
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
    /// Screenshot proof for the keyboard-shortcuts section. The Appearance
    /// dialog then shows that section alone so the frame is the rebind UI.
    pub(super) keybindings_screenshot: bool,
    /// Screenshot demo: the Appearance dialog shows only the accent, font
    /// family and power-saving sections.
    pub(super) appearance_power_screenshot: bool,
    /// Slice parity:platform-shortcuts-reference: the keyboard shortcuts
    /// reference dialog is on screen.
    pub(super) shortcuts_open: bool,
    pub(super) sticker_settings_open: bool,
    /// Settings → Appearance slice: last `(theme mode, accent)` pushed
    /// into the global component theme, so `apply_appearance` only
    /// notifies (re-renders) when something actually changed.
    pub(super) appearance_applied: Option<(ThemeMode, u32, bool, u16, String)>,
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
    /// Phase S2: storage-usage overlay (TGX Settings → Data and Storage).
    pub(super) storage_usage_open: bool,
    /// Slice S4: the Data & Storage per-network editor — the network
    /// being edited plus its draft settings (saved or discarded
    /// explicitly, never applied optimistically).
    pub(super) data_storage_editor: Option<(NetworkKind, AutoDownloadNetSettings)>,
    /// Slice S4: the "Clear cache" button is awaiting its second,
    /// confirming tap.
    pub(super) storage_confirm: Option<StorageClear>,
    /// Batch 6: the file types ticked for "Clear selected".
    pub(super) storage_selected: std::collections::BTreeSet<&'static str>,
    /// Screenshot demo: the Appearance box shows only the window and tray switches.
    pub(super) window_settings_screenshot: bool,
    /// `parity:proxy-settings`: proxy list / editor / link-confirm state.
    pub(super) proxy: super::proxy::ProxyUi,
    /// Translation: prefs (`translate_prefs.json`), the translate dialog,
    /// the bar's toast.
    pub(super) translate: super::translate_ui::TranslateUi,
    pub(super) update_state: quill::updater::UpdateState,
    pub(super) update_banner_dismissed: bool,
    /// The "No" answer of the phone-number prompt was chosen: show the note.
    pub(super) phone_change_note: bool,
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
            appearance_open: false,
            keybinding_capture: None,
            keybinding_error: None,
            keybinding_focus: cx.focus_handle(),
            keybindings_applied: false,
            keybindings_screenshot: false,
            appearance_power_screenshot: false,
            shortcuts_open: false,
            sticker_settings_open: false,
            appearance_applied: None,
            system_accent: None,
            system_accent_probed: false,
            font_picker,
            accent_picker,
            storage_usage_open: false,
            data_storage_editor: None,
            storage_confirm: None,
            storage_selected: Default::default(),
            window_settings_screenshot: false,
            proxy: Default::default(),
            translate: super::translate_ui::TranslateUi::load(),
            update_state: if demo.is_none() {
                quill::update_install::startup_state()
            } else {
                quill::updater::UpdateState::Idle
            },
            update_banner_dismissed: false,
            phone_change_note: false,
        }
    }
}
