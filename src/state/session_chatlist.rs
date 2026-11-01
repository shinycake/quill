//! Chat-list management: pins, open chat, views, ordering.
use super::*;

/// Whether `Session::main_order` / `archive_order` are stale. While a poll
/// batch is open (`deferred`), order changes only set `dirty`; closing the
/// batch sorts once.
#[derive(Debug, Default)]
pub(crate) struct ChatOrderSync {
    pub(crate) dirty: bool,
    pub(crate) deferred: bool,
    /// Stored folder tab orders, dropped whenever a folder position changes.
    pub(crate) folder_orders: super::folder_order_cache::FolderOrderCache,
}

impl Session {
    /// Demo: clear the unread marks of every chat `in_list` picks, as the
    /// server's updates do after `readChatList`, and bump the revision the
    /// unread badges recount on.
    pub fn mark_chats_read_locally(&mut self, in_list: impl Fn(&ChatSummary) -> bool) {
        for chat in self.chats.values_mut().filter(|chat| in_list(chat)) {
            chat.unread_count = 0;
            chat.is_marked_as_unread = false;
        }
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn request_download(&mut self, file_id: FileId) -> RequestId {
        let extra = self
            .requests
            .register_download(self.account_generation, file_id);
        self.media.download_extras.insert(extra.0, file_id.0);
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
            .filter(|c| Self::is_pinned_in(c, archived))
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

    fn is_pinned_in(chat: &ChatSummary, archived: bool) -> bool {
        if archived {
            chat.in_archive && chat.archive_is_pinned
        } else {
            chat.in_main_list && chat.is_pinned
        }
    }

    /// Whether `pinned_chat_ids(archived)` would hold at least two chats
    /// (the pin drag needs a pair), without sorting them.
    pub fn has_two_pinned_chats(&self, archived: bool) -> bool {
        self.chats
            .values()
            .filter(|c| Self::is_pinned_in(c, archived))
            .nth(1)
            .is_some()
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
        self.chat_order_sync.dirty = true;
        if !self.chat_order_sync.deferred {
            self.flush_chat_order();
        }
    }

    /// Open a poll batch: chat-order sorts wait for `end_update_batch`.
    pub fn begin_update_batch(&mut self) {
        self.chat_order_sync.deferred = true;
    }

    /// Close a poll batch and sort the chat lists if anything moved.
    pub fn end_update_batch(&mut self) {
        self.chat_order_sync.deferred = false;
        if self.chat_order_sync.dirty {
            self.flush_chat_order();
        }
    }

    fn flush_chat_order(&mut self) {
        self.chat_order_sync.dirty = false;
        // Sort the `(order, chat_id)` keys TDLib orders by (descending,
        // schema 1.8.67 `chatPosition.order`), never clones of whole
        // summaries.
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
        if self.search.chat_search.chat_id != Some(chat_id) {
            self.search.chat_search.close();
        }
        // Slice media-shared-gallery: switching chats closes the gallery so
        // its title and rows can't outlive the chat they belong to. `close`
        // also bumps the generation, dropping in-flight fetches for the old
        // chat. Same-chat re-select never reaches this method (see
        // `ConnectDriver::select_chat`), so the gallery survives it.
        if self.open_chat != Some(chat_id) {
            self.media.shared_media.close();
        }
        let previous = self.open_chat.replace(chat_id);
        // Phase 5.1: switching chats leaves the topic view (re-selecting the
        // chat leaves it too, so the topic's history is released here).
        if let (Some(previous), Some(topic)) = (previous, self.open_topic.take())
            && previous == chat_id
        {
            self.release_topic_history(chat_id, topic);
        }
        // ... and the Saved Messages sublist / tag filter.
        self.threads.saved.close_views();
        // ... and any comment thread that lives elsewhere.
        let left_thread_chat = self
            .threads
            .thread
            .as_ref()
            .map(|thread| thread.chat_id)
            .filter(|thread_chat| *thread_chat != chat_id);
        if left_thread_chat.is_some() {
            self.threads.thread = None;
        }
        self.view_generation.bump();
        if let Some(thread_chat) = left_thread_chat {
            self.release_chat_history(thread_chat);
        }
        if let Some(previous) = previous.filter(|previous| *previous != chat_id) {
            self.release_chat_history(previous);
        }
        self.prepare_history_view(chat_id);
        self.show_promotion_about(chat_id, std::time::Instant::now());
    }

    /// Start a fresh view of `chat_id`'s history: nothing viewed or
    /// reported yet, and the unread anchor the view opens at. Shared by
    /// the main window (`open_chat`) and a chat opened in its own window.
    pub fn prepare_history_view(&mut self, chat_id: ChatId) {
        let unread_anchor = self
            .chats
            .get(&chat_id.0)
            .and_then(ChatSummary::unread_anchor);
        self.reclaim_chat_history(chat_id);
        let view_generation = self.view_generation;
        let history = self.history_entry(chat_id);
        history.view_generation = view_generation;
        history.viewed.clear();
        history.viewing.clear();
        history.visible.clear();
        history.visible_reported = false;
        history.reader = ReaderPosition::Tail;
        // A saved-position open belongs to one view; the opener sets it again.
        history.saved_open = None;
        history.saved_open_missed = false;
        history.unread_anchor = unread_anchor;
        history.opened_with_unread = history.unread_anchor.is_some();
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
        if history.opened_with_unread {
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
    /// reported rows. Returns whether `chat_id` is on screen (the open
    /// chat or one shown in its own window).
    pub fn report_visible_messages(&mut self, chat_id: ChatId, ids: &[MessageId]) -> bool {
        if !self.chat_is_shown(chat_id) {
            return false;
        }
        let history = self.history_entry(chat_id);
        history.visible_reported = true;
        for id in ids {
            if id.0 > 0 && !history.viewed.contains(&id.0) && !history.viewing.contains(&id.0) {
                history.visible.insert(id.0);
            }
        }
        // Thread rows on screen also move the thread's own read position.
        self.thread_read_till(chat_id, ids);
        true
    }

    pub fn mark_viewed(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let history = self.history_entry(chat_id);
        for id in ids {
            history.viewing.remove(&id.0);
            history.visible.remove(&id.0);
            history.viewed.insert(id.0);
        }
    }

    /// After a successful `viewMessages` send, hold ids until TDLib ok/error.
    pub fn begin_viewing(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let history = self.history_entry(chat_id);
        for id in ids {
            history.visible.remove(&id.0);
            history.viewing.insert(id.0);
        }
    }

    pub(crate) fn commit_viewed(&mut self, chat_id: ChatId) {
        let history = self.history_entry(chat_id);
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

    /// Whether the archive holds a chat (what `ordered_archived_chats` would
    /// list is not empty), without collecting them.
    pub fn has_archived_chats(&self) -> bool {
        self.archive_order
            .iter()
            .filter_map(|id| self.chats.get(&id.0))
            .any(|chat| chat.in_archive)
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

    /// Phase 7.1: display name of a folder tab, from `updateChatFolders`.
    pub fn folder_name(&self, folder_id: i32) -> Option<&str> {
        self.chat_list
            .chat_folders
            .iter()
            .find(|f| f.id == folder_id)
            .map(|f| f.name.as_str())
    }
}
