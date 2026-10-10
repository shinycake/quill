//! Channel comments and group reply threads (tdesktop `RepliesWidget`).
use super::*;

/// Where the open thread view is in its load sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadStatus {
    /// `getMessageThread` is in flight.
    Resolving,
    /// The thread is known; the first `getMessageThreadHistory` page is in flight.
    LoadingHistory,
    Ready,
    Failed(String),
}

/// The open comment / reply thread. `chat_id` is the chat that holds the
/// replies: the linked discussion group for a channel post, the chat itself
/// for a group message. While a thread is open the session's `open_chat` is
/// that chat, so the composer, typing, read marks and menus work unchanged;
/// `origin_*` is where "back" returns to.
#[derive(Debug)]
pub struct ThreadView {
    pub origin_chat_id: ChatId,
    pub origin_message_id: MessageId,
    pub chat_id: ChatId,
    /// `messageThreadInfo.message_thread_id`; 0 until resolved.
    pub thread_id: i64,
    pub status: ThreadStatus,
    pub reply_count: i32,
    pub unread_count: i32,
    pub last_read_inbox_message_id: i64,
    /// Ids (in `chat_id`) of the messages the thread starts from: the root
    /// post that is pinned at the top of the view.
    pub root_ids: Vec<i64>,
    /// Root post and replies, keyed by message id.
    pub history: TopicHistory,
    /// The unread divider goes above the first incoming reply after this id.
    pub unread_anchor: Option<MessageId>,
    /// Bumped by "jump to root" so the view scrolls to the top again.
    pub root_jump_serial: u64,
    /// `chat_id` differs from `open_chat`: the driver still has to open it.
    pub needs_chat_switch: bool,
    /// The thread chat is open and the first history page was requested.
    pub reading_started: bool,
    /// The forum topic the thread was opened from. Sends and typing stay in
    /// that topic (`messageTopicForum`) and reply to the thread root;
    /// `None` outside forums, where a thread is its own `messageTopicThread`.
    pub forum_topic_id: Option<i32>,
}

impl ThreadView {
    pub(crate) fn resolving(origin_chat_id: ChatId, origin_message_id: MessageId) -> Self {
        Self {
            origin_chat_id,
            origin_message_id,
            chat_id: origin_chat_id,
            thread_id: 0,
            status: ThreadStatus::Resolving,
            reply_count: 0,
            unread_count: 0,
            last_read_inbox_message_id: 0,
            root_ids: Vec::new(),
            history: TopicHistory::default(),
            unread_anchor: None,
            root_jump_serial: 0,
            needs_chat_switch: false,
            reading_started: false,
            forum_topic_id: None,
        }
    }

    /// The replies live in another chat than the one the thread was opened
    /// from (a channel post's discussion group).
    pub fn is_comments(&self) -> bool {
        self.chat_id != self.origin_chat_id
    }

    /// The rows in chronological order. The root post joins the rows once
    /// the replies are loaded down to it; until then it stays in the
    /// pinned bar, so older pages prepend to the list cleanly.
    pub fn ordered(&self) -> Vec<&HistoryMessage> {
        let complete = self.history.loaded_complete;
        self.history
            .messages
            .values()
            .filter(|message| complete || !self.root_ids.contains(&message.id.0))
            .collect()
    }

    /// The root post, when it is loaded.
    pub fn root_message(&self) -> Option<&HistoryMessage> {
        self.root_ids
            .iter()
            .find_map(|id| self.history.messages.get(id))
    }

    pub fn is_root(&self, id: MessageId) -> bool {
        self.root_ids.contains(&id.0)
    }

    /// Whether an incoming message belongs to this thread.
    pub fn accepts(&self, message: &ParsedMessage) -> bool {
        if message.chat_id != self.chat_id || self.thread_id == 0 {
            return false;
        }
        message.thread_id == Some(self.thread_id)
            || message.reply_to.as_ref().is_some_and(|reply| {
                reply.is_same_chat(self.chat_id)
                    && (reply.message_id.0 == self.thread_id
                        // A reply to a reply stays in the thread (forum
                        // topics tag their messages with the topic, not the
                        // thread).
                        || (self.forum_topic_id.is_some()
                            && message.topic_id == self.forum_topic_id
                            && self.history.messages.contains_key(&reply.message_id.0)))
            })
    }

    /// Header subtitle: "N comments" for channel posts, "N replies" for
    /// groups (tdesktop `lng_comments_header` / `lng_replies_header`).
    pub fn subtitle(&self) -> String {
        thread_count_label(self.reply_count, self.is_comments())
    }
}

/// tdesktop `lng_comments_header*` / `lng_replies_header*`.
pub fn thread_count_label(count: i32, comments: bool) -> String {
    match (count, comments) {
        (0, true) => "Comments".to_owned(),
        (0, false) => "Replies".to_owned(),
        (1, true) => "1 comment".to_owned(),
        (n, true) => format!("{n} comments"),
        (1, false) => "1 reply".to_owned(),
        (n, false) => format!("{n} replies"),
    }
}

/// The label of the bar under a channel post (tdesktop
/// `lng_comments_open_count*` / `lng_comments_open_none`).
pub fn comments_bar_label(count: i32) -> String {
    match count {
        n if n <= 0 => "Leave a comment".to_owned(),
        1 => "1 comment".to_owned(),
        n => format!("{n} comments"),
    }
}

/// The label of the replies link under a group message.
pub fn replies_link_label(count: i32) -> String {
    match count {
        1 => "1 reply".to_owned(),
        n => format!("{n} replies"),
    }
}
