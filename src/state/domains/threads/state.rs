//! Forum topics, comment threads and Saved Messages: the `threads` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

impl ThreadsState {
    /// The chat list asked for this forum's topics.
    pub fn forum_row_asked(&self, chat_id: i64) -> bool {
        self.forum_topics.asked(chat_id)
    }

    /// The topic list request of this forum failed.
    pub fn forum_topics_failed(&self, chat_id: i64) -> bool {
        self.forum_topics.failed(chat_id)
    }
}

pub struct ThreadsState {
    /// Phase 5.1: cached `forumTopics` per forum chat id (first page only),
    /// with whether the chat list asked for them and how that ended.
    pub forum_topics: ForumTopicsMap,
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
    /// Supergroup ids with `supergroup.has_forum_sidebar` (schema 1.8.70).
    pub forum_sidebar_supergroups: HashSet<i64>,
    /// Slice G2: channel-comments viewer — the latest
    /// `getMessageThreadHistory` result (channel post → comment thread).
    pub thread: Option<ThreadView>,
    /// A channel's direct messages, per user, for its admins.
    pub direct: DirectMessagesTopics,
    /// Answers to the composer's new-thread requests in bot chats, until
    /// the window that asked takes its own.
    pub bot_thread_starts: Vec<BotThreadStarted>,
    /// How a topic menu action's request ended (`true`: it went through),
    /// until the window that asked takes its own.
    pub topic_action_answers: Vec<(RequestId, bool)>,
}

impl ThreadsState {
    pub(crate) fn new() -> Self {
        Self {
            forum_topics: ForumTopicsMap::default(),
            topic_histories: HashMap::new(),
            chat_view_as_topics: HashMap::new(),
            forum_topic_icons: Vec::new(),
            saved: SavedMessagesState::default(),
            forum_tabs_supergroups: HashSet::new(),
            forum_sidebar_supergroups: HashSet::new(),
            thread: None,
            direct: DirectMessagesTopics::default(),
            bot_thread_starts: Vec::new(),
            topic_action_answers: Vec::new(),
        }
    }
}
