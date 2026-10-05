//! Forum topics.
use super::*;

impl Session {
    /// Phase 5.1: enter a forum topic's view. Returns the topic's cached
    /// info, if the chat's topic list is already loaded.
    pub fn select_topic(&mut self, chat_id: ChatId, forum_topic_id: i32) -> Option<ForumTopic> {
        self.open_topic = Some(forum_topic_id);
        self.view_generation.bump();
        self.open_topic_info(chat_id)
    }

    /// Phase 5.1: leave the topic view, back to the forum's topic list.
    pub fn deselect_topic(&mut self) {
        self.open_topic = None;
        self.view_generation.bump();
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
