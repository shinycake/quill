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
    /// Unread replies, as tdesktop's `RepliesList` keeps it: TDLib's
    /// `unread_message_count` at open, then counted locally as rows are
    /// viewed (`read_till`) and live replies arrive (`note_live`).
    pub unread_count: i32,
    pub last_read_inbox_message_id: i64,
    /// `messageReplyInfo.last_message_id`: the newest reply the server
    /// knows (in `chat_id`); 0 when unknown.
    pub last_message_id: i64,
    /// Ids (in `chat_id`) of the messages the thread starts from: the root
    /// post that is pinned at the top of the view.
    pub root_ids: Vec<i64>,
    /// Root post and replies, keyed by message id. One contiguous window:
    /// `loaded_complete` once it reaches the root, `has_newer` while it
    /// stops short of the newest reply (opened at the first unread).
    pub history: TopicHistory,
    /// The window stops short of `last_message_id`: newer pages load as
    /// the reader scrolls down, and live replies wait outside it.
    pub has_newer: bool,
    /// The last newer page failed; auto-paging stops until a jump.
    pub newer_failed: bool,
    /// Bumped whenever the window is replaced, so the UI re-anchors its
    /// scroll (at the unread divider, else at the bottom).
    pub window_epoch: u64,
    /// The window was reset by the reducer (an own send while newer replies
    /// were unloaded); the driver has to request the newest page.
    pub reload_needed: bool,
    /// Page requests issued for a window that was replaced since: their
    /// answers are dropped.
    pub stale_pages: HashSet<u64>,
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
            last_message_id: 0,
            root_ids: Vec::new(),
            history: TopicHistory::default(),
            has_newer: false,
            newer_failed: false,
            window_epoch: 0,
            reload_needed: false,
            stale_pages: HashSet::new(),
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

    /// Whether any reply (not the root) is loaded.
    pub fn has_reply_rows(&self) -> bool {
        self.history
            .messages
            .keys()
            .any(|id| !self.root_ids.contains(id))
    }

    /// Newest server message in the window (pending sends excluded: their
    /// temporary ids are not positions in the thread).
    pub fn newest_loaded_id(&self) -> Option<MessageId> {
        self.history
            .messages
            .values()
            .rev()
            .find(|message| message.id.0 > 0 && !message.pending && !message.failed)
            .map(|message| message.id)
    }

    /// Recompute `has_newer` after a page landed: the window stops short of
    /// the newest reply the server reported.
    pub(crate) fn refresh_has_newer(&mut self) {
        self.has_newer = self.last_message_id > 0
            && self
                .newest_loaded_id()
                .is_some_and(|newest| newest.0 < self.last_message_id);
    }

    /// Incoming replies in the window newer than `till` (the root and own
    /// messages never count).
    fn incoming_after(&self, till: i64) -> usize {
        self.history
            .messages
            .values()
            .filter(|message| {
                message.id.0 > till
                    && message.id.0 > 0
                    && !message.is_outgoing
                    && !self.root_ids.contains(&message.id.0)
            })
            .count()
    }

    /// Rows up to `id` were viewed (or the server reported them read):
    /// advance the read position and recount the unread replies the way
    /// tdesktop's `RepliesList::computeUnreadCountLocally` does — by the
    /// loaded rows when the window reaches the newest reply, else by
    /// subtracting the rows just read. Returns whether anything changed.
    pub(crate) fn read_till(&mut self, id: i64) -> bool {
        if id <= self.last_read_inbox_message_id {
            return false;
        }
        let was = self.last_read_inbox_message_id;
        self.last_read_inbox_message_id = id;
        if self.has_newer {
            let read_now = self.incoming_after(was) - self.incoming_after(id);
            self.unread_count = (self.unread_count - read_now as i32).max(0);
        } else {
            self.unread_count = self.incoming_after(id) as i32;
        }
        true
    }

    /// A reply arrived live (update or own send). Inside a complete window
    /// it joins the rows; while newer replies are still unloaded an
    /// incoming reply only moves the counters, and an own send replaces the
    /// window so the newest page shows it (tdesktop `finishSending` →
    /// `showAtEnd`). Returns whether the row was stored.
    pub(crate) fn note_live(&mut self, row: HistoryMessage) -> bool {
        let incoming = !row.is_outgoing;
        if row.id.0 > 0 {
            self.last_message_id = self.last_message_id.max(row.id.0);
            if incoming
                && row.id.0 > self.last_read_inbox_message_id
                && !self.history.messages.contains_key(&row.id.0)
            {
                self.unread_count += 1;
            }
        }
        if self.has_newer {
            if incoming {
                return false;
            }
            self.reset_window();
        }
        self.history.upsert(row);
        true
    }

    /// Drop the loaded replies so the newest page can load; the root rows
    /// stay (they are the pinned bar), own sends still in flight stay (they
    /// belong to the newest end, as tdesktop's `appendClientSideMessages`),
    /// the counters stay.
    pub(crate) fn reset_window(&mut self) {
        let root_ids = &self.root_ids;
        self.history
            .messages
            .retain(|id, _| *id < 0 || root_ids.contains(id));
        self.history.loaded_complete = false;
        self.history.next_from_message_id = MessageId(0);
        self.has_newer = false;
        self.newer_failed = false;
        self.reload_needed = true;
        self.window_epoch = self.window_epoch.wrapping_add(1);
    }
}

/// The context-menu entry of a group message with replies (tdesktop
/// `lng_replies_view` / `lng_replies_view_thread`).
pub fn replies_menu_label(count: i32) -> String {
    match count {
        n if n <= 0 => "View Thread".to_owned(),
        1 => "View 1 Reply".to_owned(),
        n => format!("View {n} Replies"),
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
