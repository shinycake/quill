//! Notifications: OS clicks, sounds, mute and auto-delete menus, defaults.
#![deny(clippy::indexing_slicing)]

use super::notification_settings::SoundPickerTarget;
use super::*;
use quill::ids::ChatId;
use quill::telegram::envelope::NotificationSettingsScope;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;

pub(crate) struct NotifyUi {
    /// Phase 8.1: chat ids whose OS notification was clicked (set by the
    /// notification worker threads); the next render focuses the chat.
    pub(super) notify_clicks: Arc<Mutex<Vec<(ChatId, quill::notify::NotificationAction)>>>,
    /// Phase 8.1: in-flight OS notification workers; capped so a message
    /// burst cannot stack threads.
    pub(super) notify_inflight: Arc<AtomicUsize>,
    /// The message each chat's shown notification announces: an inline
    /// reply typed into it answers that message
    /// (`quill::notify::plan_inline_reply`).
    pub(super) reply_targets: std::collections::HashMap<ChatId, quill::ids::MessageId>,
    pub(super) notification_sounds: super::audio::NotificationSounds,
    /// Redraws the TDLib poll asked for, batched by urgency
    /// (`notifications::PolledRedraw`).
    pub(super) polled_redraw: super::notifications::PolledRedraw,
    /// tdesktop Mute submenu (1 hour / 8 hours / 2 days / Forever).
    pub(super) mute_menu_open: bool,
    /// The chat the Mute panel is for when it was opened from a chat row's
    /// menu; `None` means the open chat.
    pub(super) mute_chat: Option<ChatId>,
    /// The Mute submenu's "Custom..." duration row is expanded.
    pub(super) mute_custom_open: bool,
    /// The days / hours / minutes fields of the custom mute duration
    /// (tdesktop `ChooseTimeWidget`), made when the panel opens.
    pub(super) mute_time: Option<super::notification_settings::MuteTimeInputs>,
    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header (`setChatMessageAutoDeleteTime`).
    pub(super) ttl_picker_open: bool,
    /// Parity slice: the notifications panel's sound picker sub-view is open.
    pub(super) notif_sound_picker_open: bool,
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
    /// Quill's own notification windows (`notify_windows`).
    pub(super) windows: super::notify_windows::NotifyWindows,
}

impl NotifyUi {
    pub(super) fn new(audio_output: &super::audio::SharedOutput) -> Self {
        Self {
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notify_inflight: Arc::new(AtomicUsize::new(0)),
            reply_targets: std::collections::HashMap::new(),
            notification_sounds: super::audio::NotificationSounds::new(audio_output.clone()),
            polled_redraw: super::notifications::PolledRedraw::new(std::time::Instant::now()),
            mute_menu_open: false,
            mute_chat: None,
            mute_custom_open: false,
            mute_time: None,
            ttl_picker_open: false,
            notif_sound_picker_open: false,
            tone_upload_target: None,
            story_sound_picker_open: false,
            defaults_sound_picker: None,
            defaults_exceptions_scope: None,
            notifications_confirm: None,
            volume_slider: None,
            scope_volume_sliders: std::collections::HashMap::new(),
            chat_volume_sliders: std::collections::HashMap::new(),
            windows: super::notify_windows::NotifyWindows::new(),
        }
    }
}
