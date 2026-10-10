//! mute menu, notification sounds, badge prefs, scope/reaction settings.

use super::app::QuillApp;
use super::chat_theme::{danger, danger_bg};
use super::demo::{demo_file_json, demo_media_allowlist};
use super::dialogs::NotificationsConfirm;
use super::notifications::notification_settings_json;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::notify::NotificationSoundKind;
use quill::settings::BadgePrefs;
use quill::state::{ChatSummary, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    ChatKind, ChatNotificationSettings, MUTE_FOR_1_HOUR, MUTE_FOR_2_DAYS, MUTE_FOR_8_HOURS,
    MUTE_FOREVER, NotificationSettingsScope, NotificationSound, ReactionNotificationSettings,
    ReactionNotificationSource, ScopeNotificationSettings,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

mod fixtures;
mod global;
mod mute_menu;
mod reactions;
mod scope;
mod sounds;
mod ttl_picker;

pub(super) use fixtures::{apply_ready_folder_badges, apply_ready_notification_sound};

/// Phase 8.1: cap on concurrent OS-notification worker threads (`notify-send
/// --wait` blocks until dismissal). Excess bursts are dropped, not stacked.
pub(super) const MAX_OS_NOTIFICATION_THREADS: usize = 8;

/// Parity slice: cap for concurrent `quill-sound` player threads.
/// Parity slice: which settings object a sound-picker choice applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SoundPickerTarget {
    Chat(ChatId),
    /// Parity slice (`parity:stories-notify-settings`): per-chat story
    /// sound (`story_sound_id`), separate from the message sound.
    ChatStory(ChatId),
    Scope(NotificationSettingsScope),
    /// Parity slice: `reactionNotificationSettings` (TDLib 1.8.67, line 3396).
    Reaction,
}

/// Parity slice: a sound choice in the picker UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SoundChoice {
    /// App default tone (chat `use_default_sound`, scope `sound_id = -1`).
    Default,
    /// No sound (chat/scope `sound_id = 0`).
    Disabled,
    /// A saved notification sound id.
    Custom(i64),
}

/// Parity slice: which reaction-notification source a preset row applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReactionSourceKind {
    Message,
    Story,
    PollVote,
}
