//! Notifications page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page, Platforms};

const P: Page = Page::Notifications;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("notifications.mute", P, keys::Q_PREFS_MUTE_FOR_2)
        .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS)
        .keywords(&["unmute", "silence", "private chats", "groups"]),
    Entry::new(
        "notifications.preview",
        P,
        keys::Q_PREFS_SHOW_MESSAGE_PREVIEW,
    )
    .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS)
    .keywords(&["text", "content"]),
    Entry::new(
        "notifications.mentions",
        P,
        keys::Q_PREFS_NOTIFY_ABOUT_MENTIONS,
    )
    .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS)
    .keywords(&["@"]),
    Entry::new(
        "notifications.pinned",
        P,
        keys::Q_PREFS_NOTIFY_ABOUT_PINNED_MESSAGES,
    )
    .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS),
    Entry::new(
        "notifications.sound",
        P,
        keys::Q_SETTINGS_NOTIFICATIONS_SOUND,
    )
    .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS)
    .keywords(&["tone", "ringtone", "chime"]),
    Entry::new(
        "notifications.story_mute",
        P,
        keys::Q_PREFS_MUTE_STORY_NOTIFICATIONS,
    )
    .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS),
    Entry::new(
        "notifications.story_poster",
        P,
        keys::Q_PREFS_SHOW_STORY_POSTER,
    )
    .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS),
    Entry::new(
        "notifications.exceptions",
        P,
        keys::Q_SETTINGS_NOTIFICATIONS_EXCEPTIONS,
    )
    .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS)
    .keywords(&["per chat", "reset to default"]),
    Entry::new(
        "notifications.volume_by_chat_type",
        P,
        keys::Q_SETTINGS_NOTIFICATIONS_VOLUME_BY_CHAT_TYPE,
    )
    .section(keys::Q_SETTINGS_NOTIFICATIONS_SEC_CHATS)
    .keywords(&["loudness"]),
    Entry::new(
        "notifications.message_reactions",
        P,
        keys::Q_PREFS_MESSAGE_REACTIONS,
    )
    .section(keys::NOTIFICATION_REACTIONS)
    .keywords(&["likes"]),
    Entry::new(
        "notifications.story_reactions",
        P,
        keys::Q_PREFS_STORY_REACTIONS,
    )
    .section(keys::NOTIFICATION_REACTIONS),
    Entry::new(
        "notifications.poll_votes",
        P,
        keys::NOTIFICATION_REACTIONS_POLL_VOTES,
    )
    .section(keys::NOTIFICATION_REACTIONS),
    Entry::new(
        "notifications.reaction_sender",
        P,
        keys::Q_PREFS_SHOW_SENDER_AND_EMOJI_IN,
    )
    .section(keys::NOTIFICATION_REACTIONS),
    Entry::new(
        "notifications.reaction_sound",
        P,
        keys::Q_SETTINGS_NOTIFICATIONS_REACTION_SOUND,
    )
    .section(keys::NOTIFICATION_REACTIONS),
    Entry::new(
        "notifications.badge_muted",
        P,
        keys::Q_PREFS_INCLUDE_MUTED_CHATS,
    )
    .section(keys::Q_PREFS_APP_BADGE_COUNTER)
    .keywords(&["unread counter", "tray badge"]),
    Entry::new(
        "notifications.badge_archived",
        P,
        keys::Q_PREFS_INCLUDE_ARCHIVED_CHATS,
    )
    .section(keys::Q_PREFS_APP_BADGE_COUNTER)
    .keywords(&["unread counter"]),
    Entry::new(
        "notifications.badge_folders",
        P,
        keys::SETTINGS_INCLUDE_MUTED_FOLDERS,
    )
    .section(keys::Q_PREFS_APP_BADGE_COUNTER),
    Entry::new(
        "notifications.badge_count_messages",
        P,
        keys::Q_PREFS_COUNT_UNREAD_MESSAGES_OFF_COUNT,
    )
    .section(keys::Q_PREFS_APP_BADGE_COUNTER)
    .keywords(&["unread chats"]),
    Entry::new(
        "notifications.all_accounts",
        P,
        keys::Q_OPS_SHOW_NOTIFICATIONS_FROM,
    )
    .keywords(&["all accounts", "multiple accounts"]),
    Entry::new("notifications.desktop", P, keys::SETTINGS_DESKTOP_NOTIFY)
        .keywords(&["banner", "popup", "alerts"]),
    Entry::new(
        "notifications.sender_name",
        P,
        keys::Q_PREFS_SHOW_SENDER_NAME,
    )
    .section(keys::SETTINGS_DESKTOP_NOTIFY),
    Entry::new(
        "notifications.message_text",
        P,
        keys::Q_PREFS_SHOW_MESSAGE_TEXT,
    )
    .section(keys::SETTINGS_DESKTOP_NOTIFY),
    Entry::new(
        "notifications.photo_previews",
        P,
        keys::Q_PREFS_SHOW_PHOTO_PREVIEWS,
    )
    .section(keys::SETTINGS_DESKTOP_NOTIFY)
    .keywords(&["images", "pictures", "thumbnail"]),
    Entry::new(
        "notifications.native",
        P,
        keys::SETTINGS_USE_NATIVE_NOTIFICATIONS,
    )
    .section(keys::SETTINGS_NATIVE_TITLE)
    .keywords(&["system notifications", "notification center"])
    .only(Platforms::MACOS.or(Platforms::LINUX)),
    Entry::new(
        "notifications.native_windows",
        P,
        keys::SETTINGS_USE_WINDOWS,
    )
    .section(keys::SETTINGS_NATIVE_TITLE)
    .keywords(&["action center", "system notifications"])
    .only(Platforms::WINDOWS),
    Entry::new("notifications.focus_mode", P, keys::SETTINGS_SKIP_IN_FOCUS)
        .section(keys::SETTINGS_NATIVE_TITLE)
        .keywords(&["do not disturb", "focus assist"])
        .only(Platforms::WINDOWS),
    Entry::new(
        "notifications.display",
        P,
        keys::SETTINGS_NOTIFICATIONS_DISPLAY,
    )
    .section(keys::SETTINGS_NATIVE_TITLE)
    .keywords(&["monitor", "screen"]),
    Entry::new(
        "notifications.position",
        P,
        keys::SETTINGS_NOTIFICATIONS_POSITION,
    )
    .section(keys::SETTINGS_NATIVE_TITLE)
    .keywords(&["corner"]),
    Entry::new("notifications.count", P, keys::SETTINGS_NOTIFICATIONS_COUNT)
        .section(keys::SETTINGS_NATIVE_TITLE)
        .keywords(&["how many"]),
    Entry::new(
        "notifications.attention_dock",
        P,
        keys::Q_SETTINGS_NOTIFICATIONS_ATTENTION_DOCK,
    )
    .section(keys::SETTINGS_NATIVE_TITLE)
    .keywords(&["attention", "bounce"])
    .only(Platforms::MACOS),
    Entry::new(
        "notifications.attention_taskbar",
        P,
        keys::Q_SETTINGS_NOTIFICATIONS_ATTENTION_TASKBAR,
    )
    .section(keys::SETTINGS_NATIVE_TITLE)
    .keywords(&["attention", "flash"])
    .only(Platforms::WINDOWS),
    Entry::new(
        "notifications.attention_window",
        P,
        keys::Q_SETTINGS_NOTIFICATIONS_ATTENTION_WINDOW,
    )
    .section(keys::SETTINGS_NATIVE_TITLE)
    .keywords(&["attention", "urgent"])
    .only(Platforms::LINUX),
    Entry::new(
        "notifications.contact_joined",
        P,
        keys::SETTINGS_EVENTS_JOINED,
    )
    .section(keys::SETTINGS_EVENTS_TITLE)
    .keywords(&["new contact", "registered"]),
    Entry::new("notifications.in_app_sounds", P, keys::Q_PREFS_PLAY_SOUNDS)
        .section(keys::Q_PREFS_IN_APP_SOUNDS)
        .keywords(&["mute sounds", "audio"]),
    Entry::new("notifications.sound_volume", P, keys::Q_PREFS_SOUND_VOLUME)
        .section(keys::Q_PREFS_IN_APP_SOUNDS)
        .keywords(&["loudness", "call alerts"]),
    Entry::new(
        "notifications.reset_all",
        P,
        keys::Q_SETTINGS_NOTIFICATIONS_RESET_ALL,
    )
    .keywords(&["defaults", "restore"]),
];
