//! Notifications as the account on screen sees them: the mute, sound and
//! auto-delete menus, the defaults boxes and their sliders. Delivery (OS
//! clicks, sounds, Quill's notification windows) is the app's
//! (`notify_delivery_state`).
#![deny(clippy::indexing_slicing)]

use super::notification_settings::SoundPickerTarget;
use super::*;
use quill::ids::ChatId;
use quill::telegram::envelope::NotificationSettingsScope;

/// The part of the Mute panel that is expanded below the presets: one at a
/// time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MuteSection {
    #[default]
    Collapsed,
    /// The "Custom..." duration row.
    Custom,
    /// The notification sound picker.
    SoundPicker,
    /// The story sound picker.
    StorySoundPicker,
}

/// The open Mute panel (tdesktop Mute submenu: 1 hour / 8 hours / 2 days /
/// Forever), for the chat it was opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MuteBox {
    /// The chat the panel is for, set when it opens (from the header or a
    /// chat row's menu): `None` with no chat open.
    pub(super) chat: Option<ChatId>,
    pub(super) section: MuteSection,
}

/// The per-chat settings box open from a menu: the Mute panel, the
/// auto-delete picker or the Add to folder box, one at a time. One value,
/// so opening one is the other closing and a fourth box has nothing to
/// remember to clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ChatSettingsBox {
    #[default]
    None,
    Mute(MuteBox),
    /// Self-destruct / auto-delete timer picker (`setChatMessageAutoDeleteTime`),
    /// open for this chat.
    Ttl(ChatId),
    /// The Add to folder box, with the chat it was opened for.
    Folders(ChatId),
}

pub(crate) struct NotifyUi {
    /// The chat settings box that is open, if any.
    pub(super) chat_box: ChatSettingsBox,
    /// The days / hours / minutes fields of the custom mute duration
    /// (tdesktop `ChooseTimeWidget`), made when the panel opens.
    pub(super) mute_time: Option<super::notification_settings::MuteTimeInputs>,
    /// Parity slice: which defaults-dialog section's sound picker is
    /// expanded (`None` = all collapsed).
    pub(super) defaults_sound_picker: Option<SoundPickerTarget>,
    /// Parity slice: which scope section's notification exceptions list is
    /// expanded in the defaults dialog (`None` = all collapsed).
    pub(super) defaults_exceptions_scope: Option<NotificationSettingsScope>,
    /// Parity slice: pending "Reset all" confirmation on the notification
    /// defaults dialog (`resetAllNotificationSettings` wipes every
    /// notification customization with no undo, so it gates behind an
    /// explicit confirm).
    pub(super) notifications_confirm: Option<NotificationsConfirm>,
    /// Notification sound volume slider (`notification_settings/volume.rs`),
    /// created when the defaults dialog first renders.
    pub(super) volume_slider: Option<gpui_kit::Entity<gpui_kit::component::slider::SliderState>>,
    /// Per-scope sound volume sliders, created when the defaults dialog
    /// first renders (`notification_settings/scoped_volume.rs`).
    pub(super) scope_volume_sliders: std::collections::HashMap<
        (String, NotificationSettingsScope),
        gpui_kit::Entity<gpui_kit::component::slider::SliderState>,
    >,
    /// Per-chat sound volume sliders of the notification panel, one per
    /// (account, chat), created when the panel opens.
    pub(super) chat_volume_sliders: std::collections::HashMap<
        (String, ChatId),
        gpui_kit::Entity<gpui_kit::component::slider::SliderState>,
    >,
}

impl NotifyUi {
    /// Whether any sound picker (the chat panel's, its story-sound one or a
    /// defaults section's) is open: the only place a preview is waited for.
    pub(super) fn sound_picker_open(&self) -> bool {
        matches!(
            self.mute_section(),
            MuteSection::SoundPicker | MuteSection::StorySoundPicker
        ) || self.defaults_sound_picker.is_some()
    }

    /// The open Mute panel.
    pub(super) fn mute_box(&self) -> Option<MuteBox> {
        match self.chat_box {
            ChatSettingsBox::Mute(open) => Some(open),
            _ => None,
        }
    }

    /// The chat the auto-delete picker is open for.
    pub(super) fn ttl_picker(&self) -> Option<ChatId> {
        match self.chat_box {
            ChatSettingsBox::Ttl(chat) => Some(chat),
            _ => None,
        }
    }

    /// The chat the Add to folder box is open for.
    pub(super) fn folder_menu_chat(&self) -> Option<ChatId> {
        match self.chat_box {
            ChatSettingsBox::Folders(chat) => Some(chat),
            _ => None,
        }
    }

    /// Open the Mute panel (the others close).
    pub(super) fn open_mute(&mut self, open: MuteBox) {
        self.chat_box = ChatSettingsBox::Mute(open);
    }

    /// Open the auto-delete picker (the others close).
    pub(super) fn open_ttl(&mut self, chat: ChatId) {
        self.chat_box = ChatSettingsBox::Ttl(chat);
    }

    /// Open the Add to folder box (the others close).
    pub(super) fn open_folder_menu(&mut self, chat: ChatId) {
        self.chat_box = ChatSettingsBox::Folders(chat);
    }

    /// Close the Mute panel; another box that is open stays.
    pub(super) fn close_mute(&mut self) {
        if self.mute_box().is_some() {
            self.chat_box = ChatSettingsBox::None;
        }
    }

    /// Close the auto-delete picker; another box that is open stays.
    pub(super) fn close_ttl(&mut self) {
        if self.ttl_picker().is_some() {
            self.chat_box = ChatSettingsBox::None;
        }
    }

    /// Close the Add to folder box; another box that is open stays.
    pub(super) fn close_folder_menu(&mut self) {
        if self.folder_menu_chat().is_some() {
            self.chat_box = ChatSettingsBox::None;
        }
    }

    /// The chat the open Mute panel is for.
    pub(super) fn mute_chat(&self) -> Option<ChatId> {
        self.mute_box().and_then(|b| b.chat)
    }

    pub(super) fn mute_section(&self) -> MuteSection {
        self.mute_box()
            .map_or(MuteSection::Collapsed, |b| b.section)
    }

    /// Expand one section of the open Mute panel (no-op when it is closed).
    pub(super) fn set_mute_section(&mut self, section: MuteSection) {
        if let ChatSettingsBox::Mute(open) = &mut self.chat_box {
            open.section = section;
        }
    }

    /// Collapse the open Mute panel's `section`, if it is the expanded one.
    pub(super) fn collapse_mute_section(&mut self, section: MuteSection) {
        if self.mute_section() == section {
            self.set_mute_section(MuteSection::Collapsed);
        }
    }

    /// Flip `section`: it opens, or collapses when it was the open one.
    pub(super) fn toggle_mute_section(&mut self, section: MuteSection) -> bool {
        let open = self.mute_section() != section;
        self.set_mute_section(if open {
            section
        } else {
            MuteSection::Collapsed
        });
        open
    }

    pub(super) fn new() -> Self {
        Self {
            chat_box: ChatSettingsBox::None,
            mute_time: None,
            defaults_sound_picker: None,
            defaults_exceptions_scope: None,
            notifications_confirm: None,
            volume_slider: None,
            scope_volume_sliders: std::collections::HashMap::new(),
            chat_volume_sliders: std::collections::HashMap::new(),
        }
    }
}

impl super::on_screen::Farewell for NotifyUi {}
