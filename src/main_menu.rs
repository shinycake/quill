//! Main menu rules that need no UI (tdesktop `window/window_main_menu.cpp`).
//!
//! tdesktop's menu opens with My Profile, then New Group / New Channel,
//! Contacts, Calls, Saved Messages, Settings and a Night Mode switch.
//! Quill shows its entries as a filterable icon grid and leaves out the
//! ones the window already shows elsewhere ([`Entry`] says where).
//! The switch flips between the day and night theme in one step, which is
//! what `night_mode_toggle` decides from the theme in effect.
#![deny(clippy::indexing_slicing)]

use crate::settings::{AutoNight, ThemeChoice};

/// One tile of the main menu grid, in display order.
///
/// Left out on purpose, because the window already shows them:
/// Contacts and Calls (the chat list's tabs), Saved Messages (pinned in
/// the chat list, and in search), New story (the "+" of the stories
/// strip), New supergroup (tdesktop has only New Group; TDLib upgrades a
/// group as it grows), New secret chat (a contact's profile menu, as in
/// tdesktop) and New Community (a button in the Communities box).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    MyProfile,
    MyStories,
    EmojiStatus,
    NewGroup,
    NewChannel,
    Communities,
    MyGroups,
    MyChannels,
    Downloads,
    Archive,
    ArchiveToList,
    MarkRead,
    Accounts,
    Settings,
}

/// The grid's tiles; the archive pair only while the archive lives in
/// the menu (tdesktop `archiveInMainMenu`).
pub fn entries(archive_in_menu: bool) -> Vec<Entry> {
    use Entry::*;
    [
        MyProfile,
        MyStories,
        EmojiStatus,
        NewGroup,
        NewChannel,
        Communities,
        MyGroups,
        MyChannels,
        Downloads,
        Archive,
        ArchiveToList,
        MarkRead,
        Accounts,
        Settings,
    ]
    .into_iter()
    .filter(|e| archive_in_menu || !matches!(e, Archive | ArchiveToList))
    .collect()
}

/// Whether a tile's label matches the filter field: every word of the
/// query starts a word of the label, ignoring case ("my ch" finds
/// "My Channels", "read" finds "Mark all chats as read").
pub fn matches_filter(label: &str, query: &str) -> bool {
    let words: Vec<String> = label.split_whitespace().map(str::to_lowercase).collect();
    query
        .split_whitespace()
        .map(str::to_lowercase)
        .all(|q| words.iter().any(|w| w.starts_with(&q)))
}

/// A move of the keyboard highlight in the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Left,
    Right,
    Up,
    Down,
}

/// The highlighted tile after `step` in a grid of `len` tiles laid out
/// `columns` wide. Left/Right wrap through the rows; Up/Down stay put at
/// the edge. `None` (nothing highlighted yet) lands on the first tile.
pub fn step(current: Option<usize>, step: Step, len: usize, columns: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let Some(i) = current.filter(|i| *i < len) else {
        return Some(0);
    };
    let columns = columns.max(1);
    Some(match step {
        Step::Left => (i + len - 1) % len,
        Step::Right => (i + 1) % len,
        Step::Up => i.checked_sub(columns).unwrap_or(i),
        Step::Down => {
            let below = i + columns;
            if below < len { below } else { i }
        }
    })
}

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
    crate::lang::tr(if has_status {
        crate::lang::keys::MENU_CHANGE_STATUS
    } else {
        crate::lang::keys::MENU_SET_STATUS
    })
}

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
    fn archive_tiles_follow_the_setting() {
        assert!(!entries(false).contains(&Entry::Archive));
        let with = entries(true);
        assert!(with.contains(&Entry::Archive) && with.contains(&Entry::ArchiveToList));
        assert_eq!(with.len(), entries(false).len() + 2);
    }

    #[test]
    fn filter_matches_word_starts() {
        assert!(matches_filter("My Channels", ""));
        assert!(matches_filter("My Channels", "my ch"));
        assert!(matches_filter("Mark all chats as read", "READ"));
        assert!(!matches_filter("My Channels", "annels"));
        assert!(!matches_filter("Settings", "night"));
    }

    #[test]
    fn step_moves_through_the_grid() {
        assert_eq!(step(None, Step::Down, 5, 3), Some(0));
        assert_eq!(step(Some(0), Step::Left, 5, 3), Some(4));
        assert_eq!(step(Some(4), Step::Right, 5, 3), Some(0));
        assert_eq!(step(Some(1), Step::Down, 5, 3), Some(4));
        assert_eq!(step(Some(2), Step::Down, 5, 3), Some(2));
        assert_eq!(step(Some(4), Step::Up, 5, 3), Some(1));
        assert_eq!(step(Some(0), Step::Up, 5, 3), Some(0));
        assert_eq!(step(Some(9), Step::Up, 5, 3), Some(0));
        assert_eq!(step(Some(0), Step::Down, 0, 3), None);
    }

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
