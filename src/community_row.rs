//! A community shown as one chat-list entry ("Show as One Chat",
//! TDLib 1.8.70 `community.are_chats_collapsed`), ported from tdesktop's
//! collapsed community row (`data/data_community.cpp`,
//! `dialogs/ui/dialogs_layout.cpp`, `History::chatListBadgesState`).
//!
//! TDLib keeps every member chat in its own chat lists and has no entry
//! for the community, so the client groups them: the joined member chats
//! leave the main list and the archive, and one row stands in their
//! place, sorted by the newest of their last messages. This module is
//! the pure part: what the row says, its badge, and where it sits.
#![deny(clippy::indexing_slicing)]

use crate::chatlist_archive::ArchiveName;

/// `kShowChatNamesCount` in `data_community.cpp`.
pub const SHOWN_NAMES: usize = 20;

/// One chat of a collapsed community, as the row needs it.
#[derive(Debug, Clone)]
pub struct CommunityMemberChat {
    pub chat_id: i64,
    pub title: String,
    /// Last message date (0 when there is none).
    pub date: i32,
    /// Unread messages or marked as unread.
    pub unread: bool,
    pub muted: bool,
    pub mentioned: bool,
}

/// What the collapsed row prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityRowSummary {
    pub community_id: i64,
    pub name: String,
    /// The newest joined chats (`lastHistories`), newest first.
    pub names: Vec<ArchiveName>,
    /// Community chats the person is not in (`notInCount`): the "and N
    /// more chats" tail counts these, as tdesktop does.
    pub not_joined: usize,
    /// Joined chats with something unread (`CountInBadge::Chats`).
    pub unread_chats: usize,
    /// Every unread chat is muted: the counter is drawn grey.
    pub badge_muted: bool,
    /// An unread mention in any joined chat.
    pub mentioned: bool,
    /// `_chatsListDate`: the newest last-message date of the joined
    /// chats, which places the row in the list.
    pub date: i32,
    /// The joined chats, which the row hides from the lists.
    pub member_chat_ids: Vec<i64>,
}

impl CommunityRowSummary {
    /// `None` when the person is in none of the community's chats
    /// (`CommunityListInfo` needs `lastHistories` to be non-empty).
    pub fn build(
        community_id: i64,
        name: &str,
        joined: &[CommunityMemberChat],
        not_joined: usize,
    ) -> Option<Self> {
        if joined.is_empty() {
            return None;
        }
        let mut newest: Vec<&CommunityMemberChat> = joined.iter().collect();
        newest.sort_by_key(|chat| std::cmp::Reverse(chat.date));
        let date = newest.first().map_or(0, |chat| chat.date);
        newest.truncate(SHOWN_NAMES);
        let unread: Vec<&CommunityMemberChat> = joined.iter().filter(|c| c.unread).collect();
        Some(Self {
            community_id,
            name: name.to_owned(),
            names: newest
                .iter()
                .map(|chat| ArchiveName {
                    title: chat.title.clone(),
                    unread: chat.unread,
                })
                .collect(),
            not_joined,
            unread_chats: unread.len(),
            badge_muted: !unread.is_empty() && unread.iter().all(|chat| chat.muted),
            mentioned: joined.iter().any(|chat| chat.mentioned),
            date,
            member_chat_ids: joined.iter().map(|chat| chat.chat_id).collect(),
        })
    }

    /// The names joined by `lng_archived_last_list` ("A, B, C").
    pub fn joined_names(&self) -> String {
        self.names
            .iter()
            .map(|name| name.title.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Badge text of the unread counter; `None` when nothing is unread.
    pub fn badge(&self) -> Option<String> {
        (self.unread_chats > 0).then(|| {
            if self.unread_chats > 999 {
                format!("{}K", self.unread_chats / 1000)
            } else {
                self.unread_chats.to_string()
            }
        })
    }
}

/// One entry of the main list once collapsed communities are grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupedEntry {
    /// The chat at this index of the input list.
    Chat(usize),
    /// The community at this index of the summaries.
    Community(usize),
}

/// One chat of the main list, as the placement needs it.
#[derive(Debug, Clone, Copy)]
pub struct ListedChat {
    pub chat_id: i64,
    pub pinned: bool,
    pub date: i32,
}

/// The main list with collapsed communities grouped: their member chats
/// leave the list, and each community row goes after the pinned chats,
/// before the first chat whose last message is older than the row's
/// (tdesktop sorts the community entry by `_chatsListDate`, never among
/// the pinned ones: TDLib can't pin it).
pub fn group_main_list(
    chats: &[ListedChat],
    communities: &[CommunityRowSummary],
) -> Vec<GroupedEntry> {
    let hidden: std::collections::HashSet<i64> = communities
        .iter()
        .flat_map(|community| community.member_chat_ids.iter().copied())
        .collect();
    let mut rows: Vec<usize> = (0..communities.len()).collect();
    rows.sort_by_key(|&index| {
        std::cmp::Reverse(communities.get(index).map_or(0, |community| community.date))
    });
    let mut rows = rows.into_iter().peekable();
    let row_date = |index: usize| communities.get(index).map_or(0, |c| c.date);
    let mut out = Vec::with_capacity(chats.len() + communities.len());
    for (index, chat) in chats.iter().enumerate() {
        if hidden.contains(&chat.chat_id) {
            continue;
        }
        if !chat.pinned {
            while let Some(&row) = rows.peek() {
                if row_date(row) < chat.date {
                    break;
                }
                out.push(GroupedEntry::Community(row));
                rows.next();
            }
        }
        out.push(GroupedEntry::Chat(index));
    }
    out.extend(rows.map(GroupedEntry::Community));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(chat_id: i64, date: i32, unread: bool, muted: bool) -> CommunityMemberChat {
        CommunityMemberChat {
            chat_id,
            title: format!("Chat {chat_id}"),
            date,
            unread,
            muted,
            mentioned: false,
        }
    }

    #[test]
    fn summary_names_newest_first_and_counts_unread_chats() {
        let summary = CommunityRowSummary::build(
            42,
            "Rustaceans",
            &[
                member(1, 10, false, false),
                member(2, 30, true, true),
                member(3, 20, true, false),
            ],
            2,
        )
        .unwrap();
        assert_eq!(summary.joined_names(), "Chat 2, Chat 3, Chat 1");
        assert_eq!(summary.date, 30);
        assert_eq!(summary.unread_chats, 2);
        assert!(!summary.badge_muted);
        assert_eq!(summary.badge().as_deref(), Some("2"));
        assert_eq!(summary.not_joined, 2);
        assert!(CommunityRowSummary::build(42, "Empty", &[], 3).is_none());
    }

    #[test]
    fn the_badge_is_muted_only_when_every_unread_chat_is() {
        let summary = CommunityRowSummary::build(
            42,
            "Quiet",
            &[member(1, 10, true, true), member(2, 5, false, false)],
            0,
        )
        .unwrap();
        assert!(summary.badge_muted);
    }

    #[test]
    fn rows_replace_their_chats_and_sort_by_date_after_the_pinned() {
        let chat = |chat_id, pinned, date| ListedChat {
            chat_id,
            pinned,
            date,
        };
        let chats = [
            chat(1, true, 5),
            chat(2, false, 50),
            chat(3, false, 40),
            chat(4, false, 30),
            chat(5, false, 20),
        ];
        let rows = vec![
            CommunityRowSummary::build(
                42,
                "A",
                &[member(3, 40, false, false), member(9, 45, false, false)],
                0,
            )
            .unwrap(),
            CommunityRowSummary::build(43, "B", &[member(1, 100, false, false)], 0).unwrap(),
        ];
        assert_eq!(
            group_main_list(&chats, &rows),
            vec![
                GroupedEntry::Community(1),
                GroupedEntry::Chat(1),
                GroupedEntry::Community(0),
                GroupedEntry::Chat(3),
                GroupedEntry::Chat(4),
            ]
        );
        let old =
            vec![CommunityRowSummary::build(44, "C", &[member(8, 1, false, false)], 0).unwrap()];
        assert_eq!(
            group_main_list(&chats[..2], &old),
            vec![
                GroupedEntry::Chat(0),
                GroupedEntry::Chat(1),
                GroupedEntry::Community(0)
            ]
        );
    }
}
