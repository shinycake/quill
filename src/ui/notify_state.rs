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

pub(crate) struct NotifyUi {
    /// The Mute panel, while it is open.
    pub(super) mute_box: Option<MuteBox>,
    /// The days / hours / minutes fields of the custom mute duration
    /// (tdesktop `ChooseTimeWidget`), made when the panel opens.
    pub(super) mute_time: Option<super::notification_settings::MuteTimeInputs>,
    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header (`setChatMessageAutoDeleteTime`), open for
    /// this chat.
    pub(super) ttl_picker: Option<ChatId>,
    /// The picker an "Upload Sound" in flight was started from: the new
    /// sound is selected there.
    pub(super) tone_upload_target: Option<SoundPickerTarget>,
    /// Parity slice (`parity:stories-notify-settings`): the notifications
    /// panel's story-sound picker sub-view is open.
    pub(super) story_sound_picker_open: bool,
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
    /// The chat the open Mute panel is for.
    pub(super) fn mute_chat(&self) -> Option<ChatId> {
        self.mute_box.and_then(|b| b.chat)
    }

    pub(super) fn mute_section(&self) -> MuteSection {
        self.mute_box.map_or(MuteSection::Collapsed, |b| b.section)
    }

    /// Expand one section of the open Mute panel (no-op when it is closed).
    pub(super) fn set_mute_section(&mut self, section: MuteSection) {
        if let Some(open) = self.mute_box.as_mut() {
            open.section = section;
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
            mute_box: None,
            mute_time: None,
            ttl_picker: None,
            tone_upload_target: None,
            story_sound_picker_open: false,
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
