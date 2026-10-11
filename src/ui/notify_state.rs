//! Notifications: OS clicks, sounds, mute and auto-delete menus, defaults.

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
    pub(super) notification_sounds: super::audio::NotificationSounds,
    /// Redraws the TDLib poll asked for, batched by urgency
    /// (`notifications::PolledRedraw`).
    pub(super) polled_redraw: super::notifications::PolledRedraw,
    /// tdesktop Mute submenu (1 hour / 8 hours / 2 days / Forever).
    pub(super) mute_menu_open: bool,
    /// The Mute submenu's "Custom..." duration row is expanded.
    pub(super) mute_custom_open: bool,
    /// The custom mute duration being edited (tdesktop `ChooseTimeWidget`).
    pub(super) mute_custom: quill::mute_menu::CustomMute,
    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header (`setChatMessageAutoDeleteTime`).
    pub(super) ttl_picker_open: bool,
    /// The auto-delete picker's "Custom" stepper is expanded.
    pub(super) ttl_custom_open: bool,
    /// The custom auto-delete period being edited, in seconds.
    pub(super) ttl_custom_secs: i32,
    /// Parity slice: the notifications panel's sound picker sub-view is open.
    pub(super) notif_sound_picker_open: bool,
    /// Parity slice (`parity:stories-notify-settings`): the notifications
    /// panel's story-sound picker sub-view is open.
    pub(super) story_sound_picker_open: bool,
    /// Parity slice: scope-default notification settings dialog is open.
    pub(super) notification_defaults_open: bool,
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
}

impl NotifyUi {
    pub(super) fn new(audio_output: &super::audio::SharedOutput) -> Self {
        Self {
            notify_clicks: Arc::new(Mutex::new(Vec::new())),
            notify_inflight: Arc::new(AtomicUsize::new(0)),
            notification_sounds: super::audio::NotificationSounds::new(audio_output.clone()),
            polled_redraw: super::notifications::PolledRedraw::new(std::time::Instant::now()),
            mute_menu_open: false,
            mute_custom_open: false,
            mute_custom: quill::mute_menu::CustomMute::default(),
            ttl_picker_open: false,
            ttl_custom_open: false,
            ttl_custom_secs: 86_400,
            notif_sound_picker_open: false,
            story_sound_picker_open: false,
            notification_defaults_open: false,
            defaults_sound_picker: None,
            defaults_exceptions_scope: None,
            notifications_confirm: None,
            volume_slider: None,
        }
    }
}
