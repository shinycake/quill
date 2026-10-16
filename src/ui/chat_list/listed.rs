//! Which chats the list shows right now, and the multi-select checks that
//! still count. The checks are chat ids; a chat that leaves the view (the
//! folder or the Unread filter changed, it was archived or left) drops out
//! of the count and of every bulk action without anything pruning the set.

use super::*;
use std::collections::HashSet;

impl QuillApp {
    /// The main list's chats for the open folder and category (the Unread
    /// filter and a community narrow what the session lists). Empty on the
    /// Archived category, which shows the archive alone.
    pub(super) fn listed_main_chats(&self) -> Vec<&ChatSummary> {
        let folder = self.folders.tab;
        let filter = self.chat_list.filter;
        // Borrowed, never cloned: the list can hold thousands of chats and
        // this runs every render.
        let mut chats: Vec<&ChatSummary> = self
            .session()
            .map(|s| match folder {
                Some(folder_id) => s.ordered_folder_chats(folder_id),
                None => s.ordered_chats(),
            })
            .unwrap_or_default();
        // TGX `ChatFilter` unread predicate: unread_count > 0 or
        // marked-as-unread (`ChatFilter.java:166`), on the loaded model.
        if filter == ChatListFilter::Unread {
            chats.retain(|c| c.is_unread());
        }
        // `parity:communities-chatlist-mode`: the community's chats, from
        // the cached `communityFullInfo.chats` pack; hidden chats stay in
        // (`community_mode::community_member_ids`).
        if let (ChatListFilter::Community(community_id), Some(session)) = (filter, self.session()) {
            community_mode::retain_community_chats(
                &mut chats,
                Some(community_id),
                &session.groups.community_full_infos,
            );
        }
        if filter == ChatListFilter::Archived {
            chats.clear();
        }
        chats
    }

    /// The archived chats the list shows: on the Archived category, and
    /// the unread ones on the Unread category, outside folders.
    pub(super) fn listed_archived_chats(&self) -> Vec<&ChatSummary> {
        let filter = self.chat_list.filter;
        if self.folders.tab.is_some()
            || !matches!(filter, ChatListFilter::Archived | ChatListFilter::Unread)
        {
            return Vec::new();
        }
        let mut archived: Vec<&ChatSummary> = self
            .session()
            .map(|s| s.ordered_archived_chats())
            .unwrap_or_default();
        if filter == ChatListFilter::Unread {
            archived.retain(|c| c.is_unread());
        }
        archived
    }

    /// The checked chats that are in the list now, in id order. This is
    /// the selection: its length is the count, and the bulk actions and
    /// the select bar read it.
    pub(in crate::ui) fn selected_chat_ids(&self) -> Vec<ChatId> {
        if self.chat_list.selected.is_empty() {
            return Vec::new();
        }
        let mut ids: Vec<ChatId> = self
            .listed_main_chats()
            .into_iter()
            .chain(self.listed_archived_chats())
            .map(|chat| chat.id)
            .filter(|id| self.chat_list.selected.contains(&id.0))
            .collect();
        ids.sort_by_key(|id| id.0);
        ids
    }

    /// Multi-select mode shows while a checked chat is in the list.
    pub(in crate::ui) fn selecting_chats(&self) -> bool {
        !self.selected_chat_ids().is_empty()
    }

    /// Multi-select mode as the drawn rows see it: a checked chat is among
    /// the rows this frame laid out (`chat_list.items`).
    pub(in crate::ui) fn rows_selecting(&self) -> bool {
        !self.chat_list.selected.is_empty()
            && self.chat_list.items.iter().any(|item| {
                matches!(item, ChatListItem::Chat { id, .. } if self.chat_list.selected.contains(&id.0))
            })
    }

    /// Before a change builds on the checks, forget the ones no longer in
    /// the list so a toggle cannot revive a stale one.
    pub(super) fn drop_unlisted_selection(&mut self) {
        let keep: HashSet<i64> = self.selected_chat_ids().iter().map(|id| id.0).collect();
        self.chat_list.selected = keep;
    }
}
