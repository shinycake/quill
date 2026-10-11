//! Forum topics, comment threads and Saved Messages: the `threads` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct ThreadsState {
    /// Phase 5.1: cached `forumTopics` per forum chat id (first page only).
    pub forum_topics: HashMap<i64, Vec<ForumTopic>>,
    /// Phase 5.1: per-topic histories keyed by `(chat_id, forum_topic_id)`.
    pub topic_histories: HashMap<(i64, i32), TopicHistory>,
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
    /// Slice G2: channel-comments viewer — the latest
    /// `getMessageThreadHistory` result (channel post → comment thread).
    pub thread: Option<ThreadView>,
}

impl ThreadsState {
    pub(crate) fn new() -> Self {
        Self {
            forum_topics: HashMap::new(),
            topic_histories: HashMap::new(),
            chat_view_as_topics: HashMap::new(),
            forum_topic_icons: Vec::new(),
            saved: SavedMessagesState::default(),
            forum_tabs_supergroups: HashSet::new(),
            thread: None,
        }
    }
}
