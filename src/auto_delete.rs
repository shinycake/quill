//! Auto-delete timers for regular chats and groups, and the default timer
//! for new chats. tdesktop `menu/menu_ttl.cpp` (`TTLBox`): the header
//! menu's "Auto-Delete" item opens a picker over a fixed list of periods
//! (1-6 days, 1-3 weeks, 1-6 months of 31 days, 1 year) with a Disable
//! button when a timer is set. TDLib wants a multiple of 86400 seconds up
//! to a year for non-secret chats (`setChatMessageAutoDeleteTime`,
//! `setDefaultMessageAutoDeleteTime`).

use crate::telegram::envelope::ChatKind;

const DAY: i32 = 86_400;

/// tdesktop's `ttls` list in `TTLBox`.
pub const TTL_CHOICES: [i32; 16] = [
    DAY,
    DAY * 2,
    DAY * 3,
    DAY * 4,
    DAY * 5,
    DAY * 6,
    DAY * 7,
    DAY * 7 * 2,
    DAY * 7 * 3,
    DAY * 31,
    DAY * 31 * 2,
    DAY * 31 * 3,
    DAY * 31 * 4,
    DAY * 31 * 5,
    DAY * 31 * 6,
    DAY * 365,
];

/// The quick items of tdesktop's (older) auto-delete popup menu:
/// `lng_manage_messages_ttl_after1..4` (1 day, 1 week, 1 month).
pub const TTL_QUICK: [(&str, i32); 3] =
    [("1 day", DAY), ("1 week", DAY * 7), ("1 month", DAY * 31)];

/// Whether `secs` is a value TDLib accepts for a non-secret chat.
pub fn is_valid_regular_ttl(secs: i32) -> bool {
    secs == 0 || (secs > 0 && secs % DAY == 0 && secs <= 365 * DAY)
}

/// `Ui::FormatTTL`: "1 day", "5 days", "2 weeks", "3 months", "1 year".
pub fn format_ttl(secs: i32) -> String {
    if secs <= 0 {
        return "Off".to_string();
    }
    let days = secs / DAY;
    let (n, unit) = if secs % (DAY * 365) == 0 {
        (secs / (DAY * 365), "year")
    } else if secs % (DAY * 31) == 0 {
        (secs / (DAY * 31), "month")
    } else if secs % (DAY * 7) == 0 {
        (secs / (DAY * 7), "week")
    } else if secs % DAY == 0 {
        (days, "day")
    } else if secs % 3600 == 0 {
        (secs / 3600, "hour")
    } else {
        (secs / 60, "minute")
    };
    if n == 1 {
        format!("1 {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

/// Move `current` one step through [`TTL_CHOICES`]; `delta` is +1 or -1.
/// A value outside the list (set from another client) snaps to the nearest
/// choice in the direction of travel; the ends clamp.
pub fn step(current: i32, delta: i32) -> i32 {
    let list = &TTL_CHOICES;
    if delta >= 0 {
        list.iter()
            .copied()
            .find(|&c| c > current)
            .unwrap_or(list[list.len() - 1])
    } else {
        list.iter()
            .rev()
            .copied()
            .find(|&c| c < current)
            .unwrap_or(list[0])
    }
}

/// Facts about the chat needed to decide whether the user can edit its
/// timer.
#[derive(Debug, Clone, Copy)]
pub struct TtlFacts<'a> {
    pub kind: &'a ChatKind,
    /// Creator, or administrator with `can_change_info` (supergroups and
    /// channels).
    pub can_change_info: bool,
}

/// tdesktop shows the "Auto-Delete" item in private chats, basic groups
/// and (for admins who can change info) supergroups and channels. Basic
/// group rights are not tracked client-side, so TDLib's answer is the
/// authority there. Secret chats have their own self-destruct picker.
pub fn can_edit_regular_ttl(facts: TtlFacts<'_>) -> bool {
    match facts.kind {
        ChatKind::Private { .. } | ChatKind::BasicGroup { .. } => true,
        ChatKind::Supergroup { .. } => facts.can_change_info,
        ChatKind::Secret { .. } | ChatKind::Unknown => false,
    }
}

/// tdesktop's about line under the picker (`lng_ttl_edit_about*`).
pub fn about_line(kind: &ChatKind) -> &'static str {
    match kind {
        ChatKind::Private { .. } => {
            "Automatically delete new messages for you and this user after a certain period of time."
        }
        ChatKind::Supergroup {
            is_channel: true, ..
        } => {
            "Automatically delete new messages sent in this channel after a certain period of time."
        }
        _ => "Automatically delete new messages sent in this chat after a certain period of time.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::UserId;

    #[test]
    fn choices_are_valid_and_sorted() {
        for pair in TTL_CHOICES.windows(2) {
            assert!(pair[0] < pair[1]);
        }
        for secs in TTL_CHOICES {
            assert!(is_valid_regular_ttl(secs), "{secs}");
        }
        assert!(!is_valid_regular_ttl(3600));
        assert!(!is_valid_regular_ttl(-86_400));
        assert!(!is_valid_regular_ttl(366 * 86_400));
        assert!(is_valid_regular_ttl(0));
    }

    #[test]
    fn labels_match_tdesktop_format_ttl() {
        assert_eq!(format_ttl(0), "Off");
        assert_eq!(format_ttl(86_400), "1 day");
        assert_eq!(format_ttl(86_400 * 5), "5 days");
        assert_eq!(format_ttl(86_400 * 7), "1 week");
        assert_eq!(format_ttl(86_400 * 14), "2 weeks");
        assert_eq!(format_ttl(86_400 * 31), "1 month");
        assert_eq!(format_ttl(86_400 * 93), "3 months");
        assert_eq!(format_ttl(86_400 * 365), "1 year");
    }

    #[test]
    fn stepping_walks_the_list_and_clamps() {
        assert_eq!(step(0, 1), DAY);
        assert_eq!(step(DAY, 1), DAY * 2);
        assert_eq!(step(DAY * 6, 1), DAY * 7);
        assert_eq!(step(DAY * 365, 1), DAY * 365);
        assert_eq!(step(DAY, -1), DAY);
        assert_eq!(step(DAY * 7, -1), DAY * 6);
        // 10 days (set elsewhere) snaps to the neighbours.
        assert_eq!(step(DAY * 10, 1), DAY * 14);
        assert_eq!(step(DAY * 10, -1), DAY * 7);
    }

    #[test]
    fn who_can_edit() {
        let private = ChatKind::Private { user_id: UserId(1) };
        let group = ChatKind::BasicGroup { basic_group_id: 2 };
        let channel = ChatKind::Supergroup {
            supergroup_id: 3,
            is_channel: true,
        };
        let secret = ChatKind::Secret {
            secret_chat_id: 4,
            user_id: UserId(1),
        };
        let yes = |kind, can| {
            can_edit_regular_ttl(TtlFacts {
                kind,
                can_change_info: can,
            })
        };
        assert!(yes(&private, false));
        assert!(yes(&group, false));
        assert!(!yes(&channel, false));
        assert!(yes(&channel, true));
        assert!(!yes(&secret, true));
        assert!(!yes(&ChatKind::Unknown, true));
    }
}
