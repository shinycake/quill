//! Which chats the list shows right now, and the multi-select checks that
//! still count. The checks are chat ids, made in one view: changing the
//! folder or the category ends the selection (`set_chat_list_view`). A chat
//! that leaves the view meanwhile (archived, left) drops out of the count and
//! of every bulk action.

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

    /// Show another folder or category. The multi-select belongs to the view
    /// it was made in, so changing the view ends it (tdesktop
    /// `changeOpenedFolder` clears the selection). Every place that moves
    /// the folder tab or the category goes through here.
    pub(in crate::ui) fn set_chat_list_view(&mut self, filter: ChatListFilter, tab: Option<i32>) {
        if self.chat_list.filter != filter || self.folders.tab != tab {
            self.chat_list.selected.clear();
        }
        self.chat_list.filter = filter;
        self.folders.tab = tab;
    }

    /// The checked chats among `listed`. The one definition of what is
    /// selected: a checked chat that is not in the rows (archived, left)
    /// does not count.
    fn checked_among<'a>(
        &'a self,
        listed: impl Iterator<Item = ChatId> + 'a,
    ) -> impl Iterator<Item = ChatId> + 'a {
        listed.filter(|id| self.chat_list.selected.contains(&id.0))
    }

    /// The selection as the list was built this frame: pass the chats the
    /// render already collected instead of listing them again.
    pub(in crate::ui) fn selected_among_listed(
        &self,
        main: &[&ChatSummary],
        archived: &[&ChatSummary],
    ) -> Vec<ChatId> {
        let mut ids: Vec<ChatId> = self
            .checked_among(main.iter().chain(archived).map(|chat| chat.id))
            .collect();
        ids.sort_by_key(|id| id.0);
        ids
    }

    /// The checked chats that are in the list now, in id order. Its length
    /// is the count, and the bulk actions read it.
    pub(in crate::ui) fn selected_chat_ids(&self) -> Vec<ChatId> {
        if self.chat_list.selected.is_empty() {
            return Vec::new();
        }
        self.selected_among_listed(&self.listed_main_chats(), &self.listed_archived_chats())
    }

    /// Multi-select mode shows while a checked chat is in the list.
    pub(in crate::ui) fn selecting_chats(&self) -> bool {
        !self.selected_chat_ids().is_empty()
    }

    /// The same question for a drawn row: a checked chat is among the rows
    /// laid out (`chat_list.items`), without listing the chats again for
    /// every row.
    pub(in crate::ui) fn rows_selecting(&self) -> bool {
        let rows = self.chat_list.items.iter().filter_map(|item| match item {
            ChatListItem::Chat { id, .. } => Some(*id),
            _ => None,
        });
        self.checked_among(rows).next().is_some()
    }

    /// Before a change builds on the checks, forget the ones that left the
    /// list (archived, left) so a toggle cannot revive a stale one.
    pub(super) fn drop_unlisted_selection(&mut self) {
        let keep: HashSet<i64> = self.selected_chat_ids().iter().map(|id| id.0).collect();
        self.chat_list.selected = keep;
    }
}
