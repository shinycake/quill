//! Forum topics, comment threads and Saved Messages: the `threads` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

/// Whether a forum chat's topic list was asked for by the chat list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForumRowLoad {
    /// Asked (and perhaps answered): not asked again every poll.
    Asked,
    /// The request failed: the row reads "No chats" instead of "Loading…"
    /// (tdesktop marks the list loaded on a failure) and is not asked
    /// again.
    Failed,
}

impl ThreadsState {
    /// The chat list asked for this forum's topics.
    pub fn forum_row_asked(&self, chat_id: i64) -> bool {
        self.forum_row.contains_key(&chat_id)
    }

    /// The topic list request of this forum failed.
    pub fn forum_topics_failed(&self, chat_id: i64) -> bool {
        self.forum_row.get(&chat_id) == Some(&ForumRowLoad::Failed)
    }
}

pub struct ThreadsState {
    /// Phase 5.1: cached `forumTopics` per forum chat id (first page only).
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
    /// Where the topic list the chat list asks for (rows list recent topic
    /// titles) stands per forum chat. No entry: not asked yet.
    pub forum_row: HashMap<i64, ForumRowLoad>,
    /// Slice G2: channel-comments viewer — the latest
    /// `getMessageThreadHistory` result (channel post → comment thread).
    pub thread: Option<ThreadView>,
    /// A channel's direct messages, per user, for its admins.
    pub direct: DirectMessagesTopics,
    /// Answers to the composer's new-thread requests in bot chats, until
    /// the window that asked takes its own.
    pub bot_thread_starts: Vec<BotThreadStarted>,
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
            forum_row: HashMap::new(),
            thread: None,
            direct: DirectMessagesTopics::default(),
            bot_thread_starts: Vec::new(),
        }
    }
}
