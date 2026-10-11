//! The members list inside the group info panel (pure rules, no UI).
//!
//! Telegram Desktop's profile has a members block under the group details
//! (`info/profile/info_profile_members.cpp`): a header with the count, a
//! search button once the group has 20 members and an add button, then the
//! rows. Basic groups and small supergroups are sorted online first by
//! `ParticipantsOnlineSorter` (`boxes/peers/edit_participants_box.cpp`);
//! a bigger supergroup keeps the server's "recent" order because only a
//! slice of it is loaded. Rows carry an "owner" or "admin" pill, replaced
//! by the member's custom title when one is set. Rows past the first page
//! load as the list scrolls (first page 16, then 200 per request).

use crate::telegram::envelope::{ChannelMemberStatus, UserStatusKind};

/// tdesktop `kEnableSearchMembersAfterCount`: the search button appears
/// from this many members.
pub const SEARCH_AFTER_COUNT: i32 = 20;

/// tdesktop `chatSizeMax` default: a supergroup with more members than this
/// is not re-sorted by presence.
pub const ONLINE_SORT_MAX_MEMBERS: i32 = 200;

/// Rows drawn at first, and added each time the list reaches its end.
pub const ROWS_STEP: usize = 20;

/// Members asked from the server per page after the first.
pub const SERVER_PAGE: i32 = 200;

const DAY: i64 = 86_400;

/// `Data::SortByOnlineValue`: larger sorts first. Bots go last; an exact
/// "was online" time keeps its timestamp; the coarse buckets sit at fixed
/// distances in the past; an unknown status is the oldest.
pub fn online_sort_value(status: &UserStatusKind, is_bot: bool, now: i64) -> i64 {
    if is_bot {
        return -1;
    }
    match *status {
        UserStatusKind::Online => now.saturating_add(1),
        UserStatusKind::Offline { was_online } => i64::from(was_online.max(0)),
        UserStatusKind::Recently => now - 3 * DAY,
        UserStatusKind::LastWeek => now - 7 * DAY,
        UserStatusKind::LastMonth => now - 30 * DAY,
        UserStatusKind::Empty => 0,
    }
}

/// Whether the list is re-sorted by presence (tdesktop
/// `ParticipantsOnlineSorter::sort`): basic groups always, supergroups
/// that are not broadcast channels up to [`ONLINE_SORT_MAX_MEMBERS`].
pub fn sorts_by_online(is_basic_group: bool, is_channel: bool, member_count: i32) -> bool {
    is_basic_group || (!is_channel && member_count <= ONLINE_SORT_MAX_MEMBERS)
}

/// Stable sort, most recently seen first; ties keep the server order.
pub fn sort_online_first<T>(items: &mut [T], value: impl Fn(&T) -> i64) {
    items.sort_by_key(|item| std::cmp::Reverse(value(item)));
}

/// What the pill at the end of a row says and how it looks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberBadge {
    pub text: String,
    /// Owners and administrators get a filled pill; a plain member's custom
    /// title is just text.
    pub pill: bool,
    pub owner: bool,
}

/// tdesktop `MemberListRow::setType`: the custom title wins; otherwise the
/// owner and administrators read "owner" and "admin"; everyone else has no
/// badge.
pub fn member_badge(status: ChannelMemberStatus, tag: &str) -> Option<MemberBadge> {
    let owner = status == ChannelMemberStatus::Creator;
    let admin = owner || status == ChannelMemberStatus::Administrator;
    let tag = tag.trim();
    if !tag.is_empty() {
        return Some(MemberBadge {
            text: tag.to_string(),
            pill: admin,
            owner,
        });
    }
    let text = if owner {
        "owner"
    } else if admin {
        "admin"
    } else {
        return None;
    };
    Some(MemberBadge {
        text: text.to_string(),
        pill: true,
        owner,
    })
}

/// "1 member", "12 members" (tdesktop `lng_chat_status_members`).
pub fn members_title(count: i32) -> String {
    if count == 1 {
        "1 member".to_string()
    } else {
        format!("{count} members")
    }
}

/// Whether the header offers search.
pub fn search_available(member_count: i32) -> bool {
    member_count >= SEARCH_AFTER_COUNT
}

/// Basic groups have no server search: a member matches when every word
/// of the query starts a word of the name, or the username starts with it.
pub fn matches_member(name: &str, username: &str, query: &str) -> bool {
    let query = query.trim().trim_start_matches('@');
    if query.is_empty() {
        return true;
    }
    crate::contacts_index::matches_query(name, query)
        || (!username.is_empty() && username.to_lowercase().starts_with(&query.to_lowercase()))
}

/// What the scroll position asks the panel to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoreAction {
    Nothing,
    /// Draw this many rows from what is already loaded.
    Reveal(usize),
    /// Ask the server for the page starting at this offset.
    Fetch {
        offset: i32,
    },
}

/// Where the list stands when it is scrolled near its end (or too short to
/// scroll): reveal more loaded rows first, then ask the server, never while
/// a request is in flight or when the same offset already failed.
pub fn next_more(
    shown: usize,
    loaded: usize,
    total: i32,
    in_flight: bool,
    failed_offset: Option<i32>,
) -> MoreAction {
    if shown < loaded {
        return MoreAction::Reveal((shown + ROWS_STEP).min(loaded));
    }
    let offset = loaded as i32;
    if in_flight || offset >= total || failed_offset == Some(offset) {
        return MoreAction::Nothing;
    }
    MoreAction::Fetch { offset }
}

/// Append a fetched page, skipping members already present (a join between
/// two requests shifts the server's offsets). Returns how many were added.
pub fn merge_page<T, K: PartialEq>(
    loaded: &mut Vec<T>,
    page: Vec<T>,
    key: impl Fn(&T) -> K,
) -> usize {
    let before = loaded.len();
    for item in page {
        let id = key(&item);
        if !loaded.iter().any(|have| key(have) == id) {
            loaded.push(item);
        }
    }
    loaded.len() - before
}

#[cfg(test)]
mod tests {
    use super::{
        ChannelMemberStatus, MoreAction, UserStatusKind, matches_member, member_badge,
        members_title, merge_page, next_more, online_sort_value, search_available,
        sort_online_first, sorts_by_online,
    };

    const NOW: i64 = 1_700_000_000;

    fn value(status: UserStatusKind) -> i64 {
        online_sort_value(&status, false, NOW)
    }

    fn seen(ago: i64) -> UserStatusKind {
        UserStatusKind::Offline {
            was_online: (NOW - ago) as i32,
        }
    }

    #[test]
    fn online_beats_every_last_seen() {
        assert!(value(UserStatusKind::Online) > value(seen(0)));
        assert!(value(seen(60)) > value(UserStatusKind::Recently));
        assert!(value(UserStatusKind::Recently) > value(UserStatusKind::LastWeek));
        assert!(value(UserStatusKind::LastWeek) > value(UserStatusKind::LastMonth));
        assert!(value(UserStatusKind::LastMonth) > value(UserStatusKind::Empty));
    }

    #[test]
    fn bots_sort_after_everyone() {
        let bot = online_sort_value(&UserStatusKind::Online, true, NOW);
        assert!(bot < value(UserStatusKind::Empty));
    }

    #[test]
    fn sort_is_stable_for_ties() {
        let mut rows = [
            ("a", UserStatusKind::Recently),
            ("b", UserStatusKind::Recently),
            ("c", UserStatusKind::Online),
            ("d", UserStatusKind::Recently),
            ("e", UserStatusKind::Empty),
            ("f", UserStatusKind::Empty),
        ];
        sort_online_first(&mut rows, |(_, status)| value(*status));
        let order: Vec<&str> = rows.iter().map(|(id, _)| *id).collect();
        assert_eq!(order, ["c", "a", "b", "d", "e", "f"]);
    }

    #[test]
    fn sort_follows_a_status_change() {
        let mut rows = [("a", UserStatusKind::Online), ("b", seen(10))];
        sort_online_first(&mut rows, |(_, s)| value(*s));
        assert_eq!(rows[0].0, "a");
        rows[0].1 = seen(600);
        rows[1].1 = UserStatusKind::Online;
        sort_online_first(&mut rows, |(_, s)| value(*s));
        assert_eq!(rows[0].0, "b");
    }

    #[test]
    fn big_supergroups_and_channels_keep_server_order() {
        assert!(sorts_by_online(true, false, 5000));
        assert!(sorts_by_online(false, false, 200));
        assert!(!sorts_by_online(false, false, 201));
        assert!(!sorts_by_online(false, true, 10));
    }

    #[test]
    fn badges_follow_tdesktop() {
        let owner = member_badge(ChannelMemberStatus::Creator, "").unwrap();
        assert_eq!(
            (owner.text.as_str(), owner.pill, owner.owner),
            ("owner", true, true)
        );
        let admin = member_badge(ChannelMemberStatus::Administrator, "").unwrap();
        assert_eq!(
            (admin.text.as_str(), admin.pill, admin.owner),
            ("admin", true, false)
        );
        let titled = member_badge(ChannelMemberStatus::Administrator, " Moderator ").unwrap();
        assert_eq!((titled.text.as_str(), titled.pill), ("Moderator", true));
        let owner_titled = member_badge(ChannelMemberStatus::Creator, "Founder").unwrap();
        assert!(owner_titled.owner && owner_titled.pill);
        let tagged = member_badge(ChannelMemberStatus::Member, "Regular").unwrap();
        assert_eq!((tagged.text.as_str(), tagged.pill), ("Regular", false));
        assert_eq!(member_badge(ChannelMemberStatus::Member, ""), None);
        assert_eq!(member_badge(ChannelMemberStatus::Restricted, "  "), None);
    }

    #[test]
    fn title_and_search_threshold() {
        assert_eq!(members_title(1), "1 member");
        assert_eq!(members_title(0), "0 members");
        assert_eq!(members_title(12), "12 members");
        assert!(!search_available(19));
        assert!(search_available(20));
    }

    #[test]
    fn local_search_matches_name_words_and_username() {
        assert!(matches_member("Ada Lovelace", "", ""));
        assert!(matches_member("Ada Lovelace", "", "lov"));
        assert!(matches_member("Ada Lovelace", "", "ada lo"));
        assert!(!matches_member("Ada Lovelace", "", "bab"));
        assert!(matches_member("Ada", "countess", "@count"));
        assert!(!matches_member("Ada", "", "@count"));
    }

    #[test]
    fn paging_reveals_then_fetches() {
        assert_eq!(next_more(20, 200, 500, false, None), MoreAction::Reveal(40));
        assert_eq!(
            next_more(190, 200, 500, false, None),
            MoreAction::Reveal(200)
        );
        assert_eq!(
            next_more(200, 200, 500, false, None),
            MoreAction::Fetch { offset: 200 }
        );
        assert_eq!(next_more(200, 200, 500, true, None), MoreAction::Nothing);
        assert_eq!(next_more(500, 500, 500, false, None), MoreAction::Nothing);
        assert_eq!(
            next_more(200, 200, 500, false, Some(200)),
            MoreAction::Nothing
        );
        assert_eq!(
            next_more(200, 200, 700, false, Some(100)),
            MoreAction::Fetch { offset: 200 }
        );
    }

    #[test]
    fn merged_pages_skip_repeats() {
        let mut loaded = vec![1, 2, 3];
        assert_eq!(merge_page(&mut loaded, vec![3, 4, 5], |n| *n), 2);
        assert_eq!(loaded, [1, 2, 3, 4, 5]);
        assert_eq!(merge_page(&mut loaded, vec![5], |n| *n), 0);
    }
}
