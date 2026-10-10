//! Forum topics.
use super::*;

impl Session {
    /// Phase 5.1: enter a forum topic's view. Returns the topic's cached
    /// info, if the chat's topic list is already loaded.
    pub fn select_topic(&mut self, chat_id: ChatId, forum_topic_id: i32) -> Option<ForumTopic> {
        self.open_topic = Some(forum_topic_id);
        self.close_topic_thread_unless(Some(forum_topic_id));
        self.view_generation.bump();
        // Subsection tabs: rows seen in "All" were read as chat history;
        // seen again in the topic they are read as topic history
        // (`messageSourceForumTopicHistory`), which is what moves the
        // topic's own read position in TDLib.
        if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.viewed.clear();
            history.visible.clear();
        }
        self.open_topic_info(chat_id)
    }

    /// Where the forum's topic list sits (tdesktop `Dialogs::Widget::showForum`
    /// puts the topics where the chat list was). `window_width` is the
    /// window's width in pixels; `peek_chats` is the user's "show the chat
    /// list again" choice on a narrow window.
    pub fn forum_column(&self, window_width: f32, peek_chats: bool) -> ForumColumn {
        let Some(chat_id) = self.open_chat else {
            return ForumColumn::Hidden;
        };
        let is_forum = self
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_forum_chat());
        if !is_forum
            || !self.chat_views_as_topics(chat_id)
            || self.subsection_tabs_used_for(chat_id)
        {
            return ForumColumn::Hidden;
        }
        if window_width >= FORUM_COLUMN_COLLAPSE_BELOW {
            ForumColumn::Beside
        } else if peek_chats {
            ForumColumn::Hidden
        } else {
            ForumColumn::Replacing
        }
    }

    /// Phase 5.1: leave the topic view, back to the forum's topic list.
    pub fn deselect_topic(&mut self) {
        self.open_topic = None;
        self.close_topic_thread_unless(None);
        self.view_generation.bump();
    }

    /// A thread opened inside a forum topic belongs to that topic: leaving
    /// the topic (or choosing another) closes it.
    fn close_topic_thread_unless(&mut self, topic: Option<i32>) {
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| thread.forum_topic_id.is_some() && thread.forum_topic_id != topic)
        {
            self.thread = None;
        }
    }

    /// Phase 5.1: cached info for the open topic, if any.
    pub fn open_topic_info(&self, chat_id: ChatId) -> Option<ForumTopic> {
        let topic_id = self.open_topic?;
        self.forum_topics
            .get(&chat_id.0)?
            .iter()
            .find(|t| t.forum_topic_id == topic_id)
            .cloned()
    }

    /// Phase 5.1: topics for a forum chat, sorted by `order` descending
    /// (schema: "Topics must be sorted by the order in descending order").
    pub fn ordered_forum_topics(&self, chat_id: ChatId) -> Vec<ForumTopic> {
        let mut topics: Vec<ForumTopic> = self
            .forum_topics
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        topics.sort_by(|a, b| b.order.cmp(&a.order).then(a.name.cmp(&b.name)));
        topics
    }

    /// Phase 5.1: record `supergroup.is_forum` for the chat backed by this
    /// supergroup (via `updateSupergroup` or the `getSupergroup` response).
    /// Returns true when a chat was updated.
    pub fn set_supergroup_forum(&mut self, supergroup_id: i64, is_forum: bool) -> bool {
        let mut changed = false;
        for chat in self.chats.values_mut() {
            if matches!(
                chat.kind,
                ChatKind::Supergroup {
                    supergroup_id: id,
                    ..
                } if id == supergroup_id
            ) && chat.is_forum != Some(is_forum)
            {
                chat.is_forum = Some(is_forum);
                changed = true;
            }
        }
        changed
    }

    /// Parity slice: cache the first active username for a supergroup
    /// (`updateSupergroup` / `getSupergroup` response). An empty username is
    /// stored as an empty sentinel (not removed) so `maybe_fetch_supergroup_profile`
    /// doesn't re-send `getSupergroup` on every re-open of a username-less
    /// supergroup; server-pushed `updateSupergroup` still refreshes it.
    /// Render sites must filter empty before display.
    pub fn set_supergroup_username(&mut self, supergroup_id: i64, username: String) {
        self.supergroup_usernames.insert(supergroup_id, username);
    }

    /// Parity slice: cached first active username for a supergroup, if any.
    /// May be an empty sentinel when the supergroup has no username —
    /// callers should filter empty before rendering.
    pub fn supergroup_username(&self, supergroup_id: i64) -> Option<&str> {
        self.supergroup_usernames
            .get(&supergroup_id)
            .map(String::as_str)
    }

    /// Parity slice: the cached `chat.photo.small` file for a chat, if it
    /// has been marked downloaded (its `local.path` usable). `None` when
    /// the chat has no photo or the file is not local yet.
    pub fn chat_photo_path(&self, chat_id: ChatId) -> Option<&str> {
        let file_id = self.chats.get(&chat_id.0)?.photo_file_id?;
        self.files.get(&file_id)?.usable_path()
    }

    pub fn user_photo_path(&self, user_id: i64) -> Option<&str> {
        let file_id = self.users.get(&user_id)?.photo_small_file_id;
        self.files.get(&file_id)?.usable_path()
    }

    /// Parity slice: `chat.photo.small` file ids for every known chat that
    /// still needs a download — the driver's chat-list avatar hook. Like
    /// the history-thumb hook, this is deduped by `should_download`
    /// (in-flight + local), and the `small` variant is the cheap 160px
    /// thumbnail, so one pass over all chats stays cheap.
    pub fn chat_list_photo_file_ids(&self) -> Vec<FileId> {
        self.chats
            .values()
            .filter_map(|chat| chat.photo_file_id)
            .chain(
                self.users
                    .values()
                    .map(|user| user.photo_small_file_id)
                    .filter(|id| *id != 0),
            )
            .filter(|id| self.should_download(FileId(*id)))
            .map(FileId)
            .collect()
    }

    /// Avatar file ids that may need an automatic download now — the same
    /// answer as `chat_list_photo_file_ids`, but only for avatars whose
    /// owner or download state changed since the last call (plus one full
    /// pass after a new session or an auth change). Drains the due set.
    pub fn take_due_chat_list_photos(&mut self) -> Vec<FileId> {
        if std::mem::take(&mut self.avatar_rescan) {
            self.avatar_downloads_due.clear();
            let mut ids = self.chat_list_photo_file_ids();
            ids.sort_by_key(|id| id.0);
            ids.dedup();
            return ids;
        }
        let due = std::mem::take(&mut self.avatar_downloads_due);
        due.into_iter()
            .filter(|id| self.avatar_file_refs.contains_key(id))
            .map(FileId)
            .filter(|id| self.should_download(*id))
            .collect()
    }

    /// A chat or user avatar changed from `old` to `new` (0 / `None` =
    /// no photo): keep `avatar_file_refs` and queue the new one.
    pub(crate) fn replace_avatar(&mut self, old: Option<i32>, new: Option<i32>) {
        let old = old.filter(|id| *id != 0);
        let new = new.filter(|id| *id != 0);
        if old == new {
            return;
        }
        if let Some(old) = old
            && let Some(refs) = self.avatar_file_refs.get_mut(&old)
        {
            *refs -= 1;
            if *refs == 0 {
                self.avatar_file_refs.remove(&old);
            }
        }
        if let Some(new) = new {
            *self.avatar_file_refs.entry(new).or_default() += 1;
            self.avatar_downloads_due.insert(new);
        }
    }

    /// A file's download state changed; queue it if it is an avatar.
    pub(crate) fn note_avatar_file_changed(&mut self, file_id: i32) {
        if self.avatar_file_refs.contains_key(&file_id) {
            self.avatar_downloads_due.insert(file_id);
        }
    }

    /// Parity slice: the discussion-group chat id for a channel's
    /// "Discuss" affordance — `supergroupFullInfo.linked_chat_id` (0 =
    /// none). Only meaningful for channels.
    pub fn discussion_chat_id(&self, chat_id: ChatId) -> Option<i64> {
        let supergroup_id = match self.chats.get(&chat_id.0)?.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: true,
            } => supergroup_id,
            _ => return None,
        };
        let linked = self
            .supergroup_full_infos
            .get(&supergroup_id)?
            .linked_chat_id;
        // Only offer Discuss when the linked chat is actually known —
        // unknown ids degrade poorly (no history, no title), so hide it.
        (linked != 0 && self.chats.contains_key(&linked)).then_some(linked)
    }
}

/// Below this window width the topic column takes the chat list's place.
pub const FORUM_COLUMN_COLLAPSE_BELOW: f32 = 1100.;
/// Width of the topic column.
pub const FORUM_COLUMN_WIDTH: f32 = 280.;

/// Layout of the forum topic list next to the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForumColumn {
    /// No forum column: the topics, if any, fill the conversation pane.
    Hidden,
    /// A second column between the chat list and the conversation.
    Beside,
    /// The chat list collapses and the topics take its place.
    Replacing,
}
