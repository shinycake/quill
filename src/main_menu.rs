//! Main menu rules that need no UI (tdesktop `window/window_main_menu.cpp`).
//!
//! tdesktop's menu opens with My Profile, then New Group / New Channel,
//! Contacts, Calls, Saved Messages, Settings and a Night Mode switch.
//! The switch flips between the day and night theme in one step, which is
//! what `night_mode_toggle` decides from the theme in effect.

use crate::settings::{AutoNight, ThemeChoice};

/// What the Night Mode switch sets when it is flipped: the opposite of
/// the theme in effect, with the schedule turned off so the choice sticks
/// (tdesktop `ToggleNightMode` keeps the chosen palette the same way).
pub fn night_mode_toggle(dark_now: bool) -> (ThemeChoice, AutoNight) {
    (
        if dark_now {
            ThemeChoice::Light
        } else {
            ThemeChoice::Dark
        },
        AutoNight::Off,
    )
}

/// Whether the switch shows as on: a dark theme is in effect, whether it
/// was chosen or came from the schedule.
pub fn night_mode_on(applied_dark: Option<bool>, choice: ThemeChoice) -> bool {
    applied_dark.unwrap_or(matches!(
        choice,
        ThemeChoice::Dark | ThemeChoice::HighContrast
    ))
}

/// The label of the emoji status entry: tdesktop shows "Set Emoji Status"
/// until the account has a status, then "Change Emoji Status"
/// (`lng_menu_set_status` / `lng_menu_change_status`).
pub fn emoji_status_label(has_status: bool) -> &'static str {
    if has_status {
        "Change Emoji Status"
    } else {
        "Set Emoji Status"
    }
}

/// The "For ..." durations offered on a status, in seconds from now
/// (tdesktop's `PickUntilBox` list; "Other..." asks for hours).
pub const EMOJI_STATUS_DURATIONS: [(&str, i32); 4] = [
    ("For 1 hour", 3_600),
    ("For 2 hours", 7_200),
    ("For 8 hours", 28_800),
    ("For 2 days", 172_800),
];

/// The entry that asks for a custom number of hours.
pub const EMOJI_STATUS_OTHER: &str = "Other...";

/// `emojiStatus.expiration_date` for a status set `duration_secs` from
/// `now_secs`: `0` for no expiry, `None` when the date does not fit the
/// 32-bit TDLib field.
pub fn emoji_status_expiration(now_secs: u64, duration_secs: i32) -> Option<i32> {
    if duration_secs <= 0 {
        return Some(0);
    }
    i32::try_from(now_secs.checked_add(duration_secs as u64)?).ok()
}

/// Whether a chat belongs in the menu's "My Groups" (`channels` false) or
/// "My Channels" (`channels` true) list: ones the account created
/// (tdesktop `AddMyChannelsBox`). Only chats whose own membership is
/// known qualify.
pub fn is_my_created_chat(chat: &crate::state::ChatSummary, channels: bool) -> bool {
    use crate::telegram::envelope::{ChannelMemberStatus, ChatKind};
    if chat.my_member_status != Some(ChannelMemberStatus::Creator) {
        return false;
    }
    match chat.kind {
        ChatKind::Supergroup { is_channel, .. } => is_channel == channels,
        ChatKind::BasicGroup { .. } => !channels,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_label_follows_the_current_status() {
        assert_eq!(emoji_status_label(false), "Set Emoji Status");
        assert_eq!(emoji_status_label(true), "Change Emoji Status");
    }

    #[test]
    fn expiration_adds_the_duration_or_none() {
        assert_eq!(emoji_status_expiration(1_000, 0), Some(0));
        assert_eq!(emoji_status_expiration(1_000, 3_600), Some(4_600));
        assert_eq!(emoji_status_expiration(i32::MAX as u64, 60), None);
        assert_eq!(EMOJI_STATUS_DURATIONS[3], ("For 2 days", 172_800));
    }

    #[test]
    fn toggle_flips_and_pins_the_theme() {
        assert_eq!(
            night_mode_toggle(false),
            (ThemeChoice::Dark, AutoNight::Off)
        );
        assert_eq!(
            night_mode_toggle(true),
            (ThemeChoice::Light, AutoNight::Off)
        );
    }

    #[test]
    fn applied_theme_wins_over_the_stored_choice() {
        assert!(night_mode_on(Some(true), ThemeChoice::Light));
        assert!(!night_mode_on(Some(false), ThemeChoice::Dark));
        assert!(night_mode_on(None, ThemeChoice::HighContrast));
        assert!(!night_mode_on(None, ThemeChoice::Light));
    }
}

#[cfg(test)]
mod created_chat_tests {
    use super::is_my_created_chat;
    use crate::ids::ChatId;
    use crate::state::placeholder_chat;
    use crate::telegram::envelope::{ChannelMemberStatus, ChatKind};

    #[test]
    fn only_created_groups_and_channels_are_listed() {
        let mut chat = placeholder_chat(ChatId(1));
        chat.kind = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        assert!(!is_my_created_chat(&chat, true), "membership unknown");
        chat.my_member_status = Some(ChannelMemberStatus::Administrator);
        assert!(!is_my_created_chat(&chat, true));
        chat.my_member_status = Some(ChannelMemberStatus::Creator);
        assert!(is_my_created_chat(&chat, true));
        assert!(!is_my_created_chat(&chat, false));
        chat.kind = ChatKind::BasicGroup { basic_group_id: 2 };
        assert!(is_my_created_chat(&chat, false));
        chat.kind = ChatKind::Private {
            user_id: crate::ids::UserId(3),
        };
        assert!(!is_my_created_chat(&chat, false));
    }
}
