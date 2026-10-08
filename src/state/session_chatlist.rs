//! Chat-list management: pins, open chat, views, ordering.
use super::*;

impl Session {
    pub fn request_download(&mut self, file_id: FileId) -> RequestId {
        let extra = self
            .requests
            .register_download(self.account_generation, file_id);
        self.download_extras.insert(extra.0, file_id.0);
        extra
    }

    /// Slice CL2: optimistic pinned-chat reorder for `setPinnedChats`.
    /// `new_ids` is the desired full pinned order (chat ids) of the
    /// list, highest first. The pinned chats swap `order` values among
    /// themselves so the new sequence survives `rebuild_main_order`
    /// until authoritative `updateChatPosition` orders arrive (TDLib
    /// `order` is opaque — new values can't be minted, only the
    /// existing ones permuted). Returns the previous `(chat_id,
    /// order)` pairs for rollback; returns an empty vec and changes
    /// nothing when the id set doesn't match the currently pinned set.
    /// Slice CL2: ordered pinned chat ids for a list (main or archive),
    /// highest first — the canonical order `setPinnedChats` expects.
    pub fn pinned_chat_ids(&self, archived: bool) -> Vec<i64> {
        let mut pinned: Vec<(i64, i64)> = self
            .chats
            .values()
            .filter(|c| {
                if archived {
                    c.in_archive && c.archive_is_pinned
                } else {
                    c.in_main_list && c.is_pinned
                }
            })
            .map(|c| {
                let order = if archived { c.archive_order } else { c.order };
                (c.id.0, order)
            })
            .collect();
        // Canonical current order: highest order first, matching
        // `rebuild_main_order`'s sort.
        pinned.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
        pinned.into_iter().map(|(id, _)| id).collect()
    }

    pub fn reorder_pinned_chats(&mut self, archived: bool, new_ids: &[i64]) -> Vec<(i64, i64)> {
        let current: Vec<(i64, i64)> = self
            .pinned_chat_ids(archived)
            .into_iter()
            .map(|id| {
                let order = self
                    .chats
                    .get(&id)
                    .map(|c| if archived { c.archive_order } else { c.order });
                (id, order.unwrap_or(0))
            })
            .collect();
        let mut current_ids: Vec<i64> = current.iter().map(|(id, _)| *id).collect();
        let mut new_sorted: Vec<i64> = new_ids.to_vec();
        current_ids.sort_unstable();
        new_sorted.sort_unstable();
        if current_ids != new_sorted || new_ids.is_empty() {
            return Vec::new();
        }
        let orders: Vec<i64> = current.iter().map(|(_, order)| *order).collect();
        for (id, order) in new_ids.iter().zip(orders.iter()) {
            if let Some(chat) = self.chats.get_mut(id) {
                if archived {
                    chat.archive_order = *order;
                } else {
                    chat.order = *order;
                }
            }
        }
        self.rebuild_main_order();
        current
    }

    pub(crate) fn rebuild_main_order(&mut self) {
        // Runs on every position / last-message / draft update: sort the
        // `(order, chat_id)` keys TDLib orders by (descending, schema
        // 1.8.67 `chatPosition.order`), never clones of whole summaries.
        let mut rows: Vec<(i64, ChatId)> = self
            .chats
            .values()
            .filter(|c| c.in_main_list)
            .map(|c| (c.order, c.id))
            .collect();
        rows.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(b.1.0.cmp(&a.1.0)));
        self.main_order = rows.into_iter().map(|(_, id)| id).collect();
        let mut archived: Vec<(i64, ChatId)> = self
            .chats
            .values()
            .filter(|c| c.in_archive)
            .map(|c| (c.archive_order, c.id))
            .collect();
        archived.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(b.1.0.cmp(&a.1.0)));
        self.archive_order = archived.into_iter().map(|(_, id)| id).collect();
    }

    pub fn open_chat(&mut self, chat_id: ChatId) {
        if self.chat_search.chat_id != Some(chat_id) {
            self.chat_search.close();
        }
        // Slice media-shared-gallery: switching chats closes the gallery so
        // its title and rows can't outlive the chat they belong to. `close`
        // also bumps the generation, dropping in-flight fetches for the old
        // chat. Same-chat re-select never reaches this method (see
        // `ConnectDriver::select_chat`), so the gallery survives it.
        if self.open_chat != Some(chat_id) {
            self.shared_media.close();
        }
        self.open_chat = Some(chat_id);
        // Phase 5.1: switching chats leaves the topic view.
        self.open_topic = None;
        self.view_generation.bump();
        let history = self.histories.entry(chat_id.0).or_default();
        history.view_generation = self.view_generation;
        history.viewed.clear();
        history.viewing.clear();
        history.visible.clear();
        history.visible_reported = false;
        history.unread_anchor = self
            .chats
            .get(&chat_id.0)
            .and_then(ChatSummary::unread_anchor);
        history.unread_at_open = match history.unread_anchor {
            Some(_) => self
                .chats
                .get(&chat_id.0)
                .map_or(0, |chat| chat.unread_count),
            None => 0,
        };
    }

    /// Message ids of the open history that are due for `viewMessages`
    /// and not yet sent: the rows the UI reported as shown
    /// (`report_visible_messages`), or — until the UI has reported anything
    /// this open generation — only the newest sent row. A chat opens at
    /// its bottom, and viewing that message with `force_read` reads the
    /// history up to it (Telegram X `Tdlib.markChatAsRead` views
    /// `chat.lastMessage`; its `MessagesManager.viewMessages` reports only
    /// on-screen rows). Older loaded rows are never viewed just for being
    /// loaded.
    pub fn message_ids_to_view(&self, chat_id: ChatId) -> Vec<MessageId> {
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        let due =
            |id: i64| id > 0 && !history.viewed.contains(&id) && !history.viewing.contains(&id);
        if history.visible_reported {
            return history
                .visible
                .iter()
                .copied()
                .filter(|id| due(*id))
                .map(MessageId)
                .collect();
        }
        // Opened at the first unread message: nothing is read until the
        // UI reports what is actually on screen.
        if history.unread_anchor.is_some() {
            return Vec::new();
        }
        history
            .messages
            .values()
            .rev()
            .find(|message| message.id.0 > 0 && !message.pending && !message.failed)
            .map(|message| message.id)
            .filter(|id| due(id.0))
            .into_iter()
            .collect()
    }

    /// The UI shows `ids` of the open chat `chat_id`: they become due for
    /// `viewMessages`, and from now on this open generation views only
    /// reported rows. Returns whether `chat_id` is the open chat.
    pub fn report_visible_messages(&mut self, chat_id: ChatId, ids: &[MessageId]) -> bool {
        if self.open_chat != Some(chat_id) {
            return false;
        }
        let history = self.histories.entry(chat_id.0).or_default();
        history.visible_reported = true;
        for id in ids {
            if id.0 > 0 && !history.viewed.contains(&id.0) && !history.viewing.contains(&id.0) {
                history.visible.insert(id.0);
            }
        }
        true
    }

    pub fn mark_viewed(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let history = self.histories.entry(chat_id.0).or_default();
        for id in ids {
            history.viewing.remove(&id.0);
            history.visible.remove(&id.0);
            history.viewed.insert(id.0);
        }
    }

    /// After a successful `viewMessages` send, hold ids until TDLib ok/error.
    pub fn begin_viewing(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let history = self.histories.entry(chat_id.0).or_default();
        for id in ids {
            history.visible.remove(&id.0);
            history.viewing.insert(id.0);
        }
    }

    pub(crate) fn commit_viewed(&mut self, chat_id: ChatId) {
        let history = self.histories.entry(chat_id.0).or_default();
        history.viewed.extend(history.viewing.drain());
    }

    pub(crate) fn abort_viewing(&mut self, chat_id: ChatId) {
        if let Some(history) = self.histories.get_mut(&chat_id.0) {
            // Reported rows stay due so the retry re-sends them; before any
            // report the newest row is recomputed anyway.
            if history.visible_reported {
                history.visible.extend(history.viewing.drain());
            } else {
                history.viewing.clear();
            }
        }
    }

    pub fn ordered_chats(&self) -> Vec<&ChatSummary> {
        self.main_order
            .iter()
            .filter_map(|id| self.chats.get(&id.0))
            .filter(|chat| chat.in_main_list)
            .collect()
    }

    /// Chats in `chatListArchive`, highest TDLib order first (same as main).
    pub fn ordered_archived_chats(&self) -> Vec<&ChatSummary> {
        self.archive_order
            .iter()
            .filter_map(|id| self.chats.get(&id.0))
            .filter(|chat| chat.in_archive)
            .collect()
    }

    /// What the chat list's "Archived chats" row shows (names of the
    /// newest archived chats, unread-chat badge); `None` when the archive
    /// is empty.
    pub fn archive_row_summary(&self) -> Option<crate::chatlist_archive::ArchiveRowSummary> {
        let chats: Vec<crate::chatlist_archive::ArchivedChat> = self
            .ordered_archived_chats()
            .into_iter()
            .map(|chat| crate::chatlist_archive::ArchivedChat {
                title: chat.title.clone(),
                date: chat.last_message.as_ref().map_or(0, |m| m.date),
                unread: chat.is_unread(),
            })
            .collect();
        crate::chatlist_archive::ArchiveRowSummary::build(&chats)
    }

    /// Phase 7.1: chats in `chatListFolder(folder_id)`, highest TDLib folder
    /// order first (same convention as main/archive).
    pub fn ordered_folder_chats(&self, folder_id: i32) -> Vec<&ChatSummary> {
        let mut rows: Vec<&ChatSummary> = self
            .chats
            .values()
            .filter(|chat| chat.folder_positions.contains_key(&folder_id))
            .collect();
        rows.sort_by(|a, b| {
            b.folder_positions
                .get(&folder_id)
                .cmp(&a.folder_positions.get(&folder_id))
                .then(b.id.0.cmp(&a.id.0))
        });
        rows
    }

    /// Phase 7.1: display name of a folder tab, from `updateChatFolders`.
    pub fn folder_name(&self, folder_id: i32) -> Option<&str> {
        self.chat_folders
            .iter()
            .find(|f| f.id == folder_id)
            .map(|f| f.name.as_str())
    }
}
