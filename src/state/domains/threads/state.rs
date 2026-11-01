//! Forum topics, comment threads and Saved Messages: the `threads` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct ThreadsState {
    /// Phase 5.1: cached `forumTopics` per forum chat id (first page only).
    pub forum_topics: ForumTopicsMap,
    /// Phase 5.1: per-topic histories keyed by `(chat_id, forum_topic_id)`.
    pub topic_histories: HashMap<(i64, i32), TopicHistory>,
    /// Topics whose first history page failed or timed out (error answer
    /// or the pending-request sweep): the pane shows "Couldn't load
    /// messages · Retry". Cleared by a landed page, a retry or re-selecting
    /// the topic.
    pub topic_load_failed: HashSet<(i64, i32)>,
    /// `chat.view_as_topics` / `updateChatViewAsTopics`, by chat id: a
    /// forum shown as topics, Saved Messages shown as chats.
    pub chat_view_as_topics: HashMap<i64, bool>,
    /// `getForumTopicDefaultIcons`: the custom emoji a topic may use.
    pub forum_topic_icons: Vec<StickerItem>,
    /// Saved Messages sublists, tags and the open sublist / tag filter.
    pub saved: SavedMessagesState,
    /// Subsection tabs: supergroup ids with `supergroup.has_forum_tabs`
    /// (schema 1.8.67, line 2746), from `updateSupergroup` / `getSupergroup`.
    pub forum_tabs_supergroups: HashSet<i64>,
    /// Forum chats whose topic list the chat list already asked for (rows
    /// list recent topic titles), so a failed answer is not asked again
    /// every poll.
    pub forum_row_asked: HashSet<i64>,
    /// Forum chats whose topic list request failed: the row then reads "No
    /// chats" instead of "Loading…" (tdesktop marks the list loaded on a
    /// failure) and is not asked again.
    pub forum_topics_failed: HashSet<i64>,
    /// Slice G2: channel-comments viewer — the latest
    /// `getMessageThreadHistory` result (channel post → comment thread).
    pub thread: Option<ThreadView>,
}

impl ThreadsState {
    pub(crate) fn new() -> Self {
        Self {
            forum_topics: ForumTopicsMap::default(),
            topic_histories: HashMap::new(),
            topic_load_failed: HashSet::new(),
            chat_view_as_topics: HashMap::new(),
            forum_topic_icons: Vec::new(),
            saved: SavedMessagesState::default(),
            forum_tabs_supergroups: HashSet::new(),
            forum_row_asked: HashSet::new(),
            forum_topics_failed: HashSet::new(),
            thread: None,
        }
    }
}
