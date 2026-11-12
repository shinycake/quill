//! Settings state: the settings window and its pages (the account's,
//! `SettingsUi`) and what the process keeps across accounts (appearance,
//! shortcuts, translation, updates: `PreferencesUi`).
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::*;

/// The settings window of the account on screen: its pages, search,
/// scrolls and the boxes they open. A field of `OnScreen`.
pub(crate) struct SettingsUi {
    /// Whether the settings window covers the chat list and the chat, and
    /// on which page or subpage.
    pub(super) nav: super::settings_window::SettingsNav,
    /// The box the settings window has open, if any: one value, since
    /// kit dialogs never stack.
    pub(super) boxes: SettingsBox,
    /// The search field and its results.
    pub(super) search: super::settings_window::SettingsSearch,
    /// The setting search just landed on.
    pub(super) arrival: Option<super::settings_window::Arrival>,
    /// Scrolls every page without a handle of its own.
    pub(super) page_scroll: ScrollHandle,
    /// Scrolls the screen in `sub`.
    pub(super) sub_scroll: ScrollHandle,
    /// Holds focus inside the settings window, so Esc steps back.
    pub(super) focus: FocusHandle,
    /// The stickers and masks manager boxes: removals, dragged order and
    /// scroll. The session has one manager, so its edits are the screen's,
    /// not the window's that opened it (the masks box opens from a chat
    /// window's photo editor): the close commits them from any window.
    pub(super) sticker_manager: super::stickers_manager::StickersManagerUi,
    pub(super) storage_scroll: super::dialogs::data_storage::StorageScroll,
    /// The automatic media download box open over Data and storage.
    pub(super) auto_download_editor: super::dialogs::auto_download::AutoDownloadEditor,
    /// Slice S4: the "Clear cache" button is awaiting its second,
    /// confirming tap.
    pub(super) storage_confirm: Option<StorageClear>,
    /// Batch 6: the file types ticked for "Clear selected".
    pub(super) storage_selected: std::collections::BTreeSet<&'static str>,
    /// `parity:proxy-settings`: proxy list / editor / link-confirm state.
    pub(super) proxy: super::proxy::ProxyUi,
    /// Release-delay slider in Settings > Calls (`ui/ptt_delay.rs`). The
    /// screen's: it is seeded from, and saves into, the call prefs of the
    /// account it was made for, which are per account.
    pub(super) ptt_delay_slider: Option<Entity<gpui_kit::component::slider::SliderState>>,
}

/// What the operating system's accent color is known to be.
#[derive(Default)]
pub(crate) enum SystemAccent {
    /// Not read yet.
    #[default]
    Unread,
    /// Linux reads it on a background thread: the color known so far, and
    /// the task waiting for the read to land (dropping it ends the wait; a
    /// second refresh while it runs does not start another).
    Reading { color: Option<u32>, _wait: Task<()> },
    /// Read: the color (0xRRGGBB) when the system reports one.
    Read(Option<u32>),
}

impl SystemAccent {
    /// The color, when read and reported.
    pub(super) fn color(&self) -> Option<u32> {
        match self {
            Self::Read(color) | Self::Reading { color, .. } => *color,
            Self::Unread => None,
        }
    }

    pub(super) fn is_unread(&self) -> bool {
        matches!(self, Self::Unread)
    }
}

/// The box the settings window shows over itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SettingsBox {
    #[default]
    None,
    /// Account > Log out confirmation.
    LogOut,
    /// Chats > "Manage sticker sets".
    Stickers,
}

impl SettingsBox {
    /// Open `which` (replacing any other box), or close it when it is the
    /// one open: closing a box never closes another.
    pub(crate) fn set(&mut self, which: SettingsBox, open: bool) {
        if open {
            *self = which;
        } else if *self == which {
            *self = SettingsBox::None;
        }
    }
}

impl SettingsUi {
    /// The settings window covers the chat list and the chat.
    pub(super) fn open(&self) -> bool {
        self.nav.is_open()
    }

    /// The page on screen. `None`: none chosen yet, so a wide window shows
    /// Account and a narrow one the page list.
    pub(super) fn page(&self) -> Option<quill::settings_catalog::Page> {
        self.nav.page()
    }

    /// A page reached from a row of `page`, with a back arrow.
    pub(super) fn sub(&self) -> Option<super::settings_window::SubPage> {
        self.nav.sub()
    }

    pub(super) fn box_open(&self, which: SettingsBox) -> bool {
        self.boxes == which
    }

    /// Open `which` (replacing any other box), or close it when it is the
    /// one open: closing a box never closes another.
    pub(super) fn set_box(&mut self, which: SettingsBox, open: bool) {
        self.boxes.set(which, open);
    }

    pub(super) fn new(cx: &mut Context<QuillApp>) -> Self {
        Self {
            nav: Default::default(),
            boxes: SettingsBox::None,
            search: Default::default(),
            arrival: None,
            page_scroll: ScrollHandle::new(),
            sub_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            sticker_manager: Default::default(),
            storage_scroll: super::dialogs::data_storage::StorageScroll::new(),
            auto_download_editor: Default::default(),
            storage_confirm: None,
            storage_selected: Default::default(),
            proxy: Default::default(),
            ptt_delay_slider: None,
        }
    }
}

/// The stickers manager commits its reordering and removals through the
/// old client when it closes (`close_stickers_manager`), and so do the
/// masks and emoji set boxes, whose flags are the session's. The page shown
/// is left as on a page change, so what it holds in app-wide state (the
/// language box, the storage sliders, an armed push-to-talk capture)
/// does not carry over to the next account.
impl super::on_screen::Farewell for SettingsUi {
    fn farewell(
        app: &mut QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        if app.settings.box_open(SettingsBox::Stickers) {
            app.close_stickers_manager(cx);
        }
        // The proxy-link box (its draft and check) is the session's.
        app.close_proxy_link(cx);
        // These boxes keep their open flags in the session, which goes to
        // the background with the account.
        let (masks, emoji, archive) = app.session().map_or((false, false, false), |session| {
            (
                session.stickers.masks.dialog_open(),
                session.stickers.emoji.open,
                session.chat_list.archive_settings_open,
            )
        });
        if masks {
            app.close_mask_sets(cx);
        }
        if emoji {
            app.close_emoji_sets(cx);
        }
        if archive {
            app.set_archive_settings_open(false);
        }
        if let Some(page) = app.settings.page() {
            app.leave_settings_page(page);
        }
    }
}

/// What Settings keeps for the process, whichever account is on screen:
/// shortcuts, the applied appearance and themes, the pickers, translation
/// prefs and its box, app updates, What's New, the advanced page and the
/// emoji set. A `QuillApp` field (`preferences`).
pub(crate) struct PreferencesUi {
    /// The rebinding capture and the keymap's stage (`ui/keybindings`).
    pub(super) keybinding: super::keybindings::KeybindingUi,
    /// Parity slice (platform-custom-keybindings): focus handle for the
    /// keystroke-capture row.
    pub(super) keybinding_focus: FocusHandle,
    /// Settings → Appearance slice: last `(theme mode, accent)` pushed
    /// into the global component theme, so `apply_appearance` only
    /// notifies (re-renders) when something actually changed.
    /// The middle value is the applied palette's fingerprint (the accent
    /// under high contrast).
    pub(super) appearance_applied: Option<(ThemeMode, u64, bool, u16, String)>,
    /// The wake for scheduled auto-night (`appearance/night_clock.rs`).
    pub(super) night_clock: super::appearance::NightClock,
    /// App themes: custom theme list, the applied palette, the editor.
    pub(super) app_theme: super::app_theme::AppThemeUi,
    /// The operating system's accent color, once read
    /// (`refresh_system_accent`).
    pub(super) system_accent: SystemAccent,
    /// The wait for the link handler's background read to land; dropping
    /// it ends the wait.
    pub(super) link_handler_watch: Option<Task<()>>,
    /// "Make Quill the default": the re-reads that follow while the
    /// system confirmation is awaited (`LinkHandlerRecheck`).
    pub(super) link_handler_recheck: super::appearance::LinkHandlerRecheck,
    /// Settings → Appearance: the searchable font family list.
    pub(super) font_picker: super::appearance_power::FontPicker,
    /// Settings → Appearance: the custom accent color field (kit
    /// `ColorSelect`), tdesktop's "custom" accent circle. Holds the last
    /// custom color; choosing one sets `appearance.accent_rgb`.
    pub(super) accent_picker: Entity<gpui_kit::component::color_picker::ColorPickerState>,
    /// Translation prefs (`translate_prefs.json`); the dialog and the
    /// bar's toast are the screen's (`OnScreen::translate`).
    pub(super) translate: super::translate_ui::TranslateUi,
    pub(super) update_state: quill::updater::UpdateState,
    /// The What's New window and the release notes it shows.
    pub(super) whats_new: super::whats_new::WhatsNewUi,
    /// Advanced and experimental settings, the secret codes, the test
    /// environment (`ui/advanced_state.rs`).
    pub(super) advanced: super::advanced_state::AdvancedUi,
    /// Settings → Chat → "Choose emoji set" and the set in use (`ui/emoji_style_ui.rs`).
    pub(super) emoji_style: super::emoji_style_ui::EmojiStyleUi,
    /// Settings → General → the Send To menu switch (`ui/appearance/send_to_menu_section.rs`).
    pub(super) send_to: super::appearance::SendToMenu,
}

impl PreferencesUi {
    pub(super) fn new(
        cx: &mut Context<QuillApp>,
        font_picker: super::appearance_power::FontPicker,
        accent_picker: Entity<gpui_kit::component::color_picker::ColorPickerState>,
        demo: Option<ScreenshotDemo>,
    ) -> Self {
        Self {
            keybinding: Default::default(),
            keybinding_focus: cx.focus_handle(),
            appearance_applied: None,
            night_clock: Default::default(),
            app_theme: Default::default(),
            system_accent: SystemAccent::Unread,
            link_handler_watch: None,
            link_handler_recheck: Default::default(),
            font_picker,
            accent_picker,
            translate: super::translate_ui::TranslateUi::load(),
            update_state: if demo.is_none() {
                quill::update_install::startup_state()
            } else {
                quill::updater::UpdateState::Idle
            },
            whats_new: Default::default(),
            advanced: super::advanced_state::AdvancedUi::new(cx, demo.is_none()),
            emoji_style: Default::default(),
            send_to: Default::default(),
        }
    }
}

#[cfg(test)]
mod box_tests {
    use super::SettingsBox;

    #[test]
    fn closing_a_box_never_closes_the_other() {
        let mut boxes = SettingsBox::None;
        boxes.set(SettingsBox::Stickers, true);
        boxes.set(SettingsBox::LogOut, false);
        assert_eq!(boxes, SettingsBox::Stickers);
        boxes.set(SettingsBox::LogOut, true);
        assert_eq!(boxes, SettingsBox::LogOut, "one box at a time");
    }
}
