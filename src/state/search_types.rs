//! Search state types: global, in-chat and topic search.
use super::*;

/// Global search (official sidebar field): recents, then `searchChats` + `searchMessages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStatus {
    Closed,
    Idle,
    Searching,
    Ready,
    Empty,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMessageHit {
    pub sender: Option<MessageSender>,
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub preview: String,
    pub is_outgoing: bool,
    pub content: MessageContent,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    pub is_pinned: bool,
    pub media_album_id: i64,
    pub reply_markup: Option<ReplyMarkup>,
    /// Phase B3: carried through from `ParsedMessage` so search hits can
    /// become history rows without losing the timer badge.
    pub self_destruct: Option<MessageSelfDestruct>,
    /// Phase B4: same carry-through for the auto-delete countdown chip.
    pub auto_delete: Option<MessageAutoDelete>,
    /// Phase D2: same carry-through for the author signature line.
    pub author_signature: Option<String>,
    /// kit Phase 4: same carry-through for the in-bubble timestamp.
    pub date: i32,
}

impl SearchMessageHit {
    pub(crate) fn from_parsed(message: &ParsedMessage) -> Self {
        Self {
            sender: message.sender,
            chat_id: message.chat_id,
            message_id: message.id,
            preview: effective_content(&message.content, message.ephemeral.as_ref()).preview(),
            is_outgoing: message.is_outgoing,
            content: message.content.clone(),
            reply_to: message.reply_to.clone(),
            forward_info: message.forward_info.clone(),
            interaction_info: message.interaction_info.clone(),
            is_pinned: message.is_pinned,
            media_album_id: message.media_album_id,
            reply_markup: message.reply_markup.clone(),
            self_destruct: message.self_destruct,
            auto_delete: message.auto_delete,
            author_signature: message.author_signature.clone(),
            date: message.date,
        }
    }

    pub(crate) fn into_history(self) -> HistoryMessage {
        HistoryMessage {
            sender: self.sender,
            id: self.message_id,
            chat_id: self.chat_id,
            is_outgoing: self.is_outgoing,
            date: self.date,
            content: self.content,
            pending: false,
            reply_to: self.reply_to,
            forward_info: self.forward_info,
            interaction_info: self.interaction_info,
            is_pinned: self.is_pinned,
            media_album_id: self.media_album_id,
            reply_markup: self.reply_markup,
            self_destruct: self.self_destruct,
            auto_delete: self.auto_delete,
            author_signature: self.author_signature,
            failed: false,
            can_retry: false,
            ephemeral: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub open: bool,
    pub query: String,
    pub generation: u64,
    pub status: SearchStatus,
    pub chat_ids: Vec<ChatId>,
    pub messages: Vec<SearchMessageHit>,
    /// Phase 7.2: `searchPublicChats` results (username/title lookup across
    /// all public chats — not just known ones). Tracked separately from
    /// `chat_ids` (offline `searchChats`) with its own done/error flags so
    /// the status waits for all three requests.
    pub public_chat_ids: Vec<ChatId>,
    /// Slice (communities-search-filter): community id picked via the
    /// search-panel filter chips (`None` = "All chats"). Feeds
    /// `searchMessagesChatTypeFilterCommunity` in `searchMessages` (set by
    /// `ConnectDriver::set_search_community_filter`). Kept across re-queries;
    /// cleared on close and query-clear.
    pub community_filter: Option<i64>,
    /// Empty-query surface: `searchRecentlyFoundChats` (official Recent).
    pub recents: bool,
    pub(crate) chats_done: bool,
    pub(crate) messages_done: bool,
    pub(crate) public_done: bool,
    pub(crate) chats_error: bool,
    pub(crate) messages_error: bool,
    pub(crate) public_error: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            generation: 0,
            status: SearchStatus::Closed,
            chat_ids: Vec::new(),
            messages: Vec::new(),
            public_chat_ids: Vec::new(),
            community_filter: None,
            recents: false,
            chats_done: false,
            messages_done: false,
            public_done: false,
            chats_error: false,
            messages_error: false,
            public_error: false,
        }
    }
}

impl SearchState {
    pub fn open_field(&mut self) {
        if self.open {
            return;
        }
        self.open = true;
        self.query.clear();
        self.recents = true;
        self.status = SearchStatus::Idle;
        self.clear_results();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.recents = false;
        self.community_filter = None;
        self.status = SearchStatus::Closed;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.recents = true;
        self.community_filter = None;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
        self.status = if self.open {
            SearchStatus::Idle
        } else {
            SearchStatus::Closed
        };
    }

    /// Empty search field: wait only for `searchRecentlyFoundChats`.
    pub fn begin_recents(&mut self) -> u64 {
        self.open = true;
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.recents = true;
        self.messages_done = true;
        self.public_done = true;
        self.generation
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.open = true;
        self.query = query.to_string();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.recents = false;
        self.generation
    }

    pub(crate) fn clear_results(&mut self) {
        self.chat_ids.clear();
        self.messages.clear();
        self.public_chat_ids.clear();
        self.chats_done = false;
        self.messages_done = false;
        self.public_done = false;
        self.chats_error = false;
        self.messages_error = false;
        self.public_error = false;
    }

    pub(crate) fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.open
    }

    pub(crate) fn accept_chats(&mut self, chat_ids: Vec<ChatId>, error: bool) {
        self.chat_ids = chat_ids;
        self.chats_done = true;
        self.chats_error = error;
        self.finish_if_complete();
    }

    pub(crate) fn accept_messages(&mut self, messages: Vec<SearchMessageHit>, error: bool) {
        self.messages = messages;
        self.messages_done = true;
        self.messages_error = error;
        self.finish_if_complete();
    }

    pub(crate) fn accept_public_chats(&mut self, chat_ids: Vec<ChatId>, error: bool) {
        self.public_chat_ids = chat_ids;
        self.public_done = true;
        self.public_error = error;
        self.finish_if_complete();
    }

    pub(crate) fn finish_if_complete(&mut self) {
        if !(self.chats_done && self.messages_done && self.public_done) {
            return;
        }
        let any = !self.chat_ids.is_empty()
            || !self.messages.is_empty()
            || !self.public_chat_ids.is_empty();
        self.status = if any {
            SearchStatus::Ready
        } else if self.chats_error || self.messages_error || self.public_error {
            SearchStatus::Failed
        } else if self.recents {
            SearchStatus::Idle
        } else {
            SearchStatus::Empty
        };
    }
}

/// Jump-to-message after an in-chat hit (Unigram `LoadMessageSliceAsync`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchJump {
    None,
    /// `getChatHistory` around the target is in flight — not yet loaded.
    Loading {
        message_id: MessageId,
    },
    Ready {
        message_id: MessageId,
    },
    /// Deleted (tombstone) or inaccessible after the around-load returned.
    Missing {
        message_id: MessageId,
    },
}

impl ChatSearchJump {
    pub fn message_id(self) -> Option<MessageId> {
        match self {
            Self::None => None,
            Self::Loading { message_id }
            | Self::Ready { message_id }
            | Self::Missing { message_id } => Some(message_id),
        }
    }

    pub fn is_ready_at(self, id: MessageId) -> bool {
        matches!(self, Self::Ready { message_id } if message_id == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchJumpNeed {
    AlreadyReady,
    Missing,
    LoadAround,
}

/// Phase 5.1: per-topic history for a forum supergroup, fetched with
/// `searchChatMessages` (`topic_id = messageTopicForum`, empty query).
/// `next_from_message_id` pages older messages the same way `foundChatMessages`
/// does for in-chat search; `loaded_complete` once a page returns
/// `next_from_message_id` 0 (or an empty page).
#[derive(Debug)]
pub struct TopicHistory {
    pub messages: BTreeMap<i64, HistoryMessage>,
    pub next_from_message_id: MessageId,
    pub loaded_complete: bool,
}

impl Default for TopicHistory {
    fn default() -> Self {
        Self {
            messages: BTreeMap::new(),
            next_from_message_id: MessageId(0),
            loaded_complete: false,
        }
    }
}

impl TopicHistory {
    pub fn ordered(&self) -> Vec<&HistoryMessage> {
        self.messages.values().collect()
    }

    /// Parity slice 4: live topic messages (incoming updates and own
    /// sends) land here when the topic is loaded. Entries are only ever
    /// created by the `GetTopicHistory` fetch — upserting into a missing
    /// topic would corrupt the paging cursor.
    pub(crate) fn upsert(&mut self, message: HistoryMessage) {
        self.messages.insert(message.id.0, message);
    }

    /// Parity slice 4: `updateMessageSendSucceeded` / `Failed` replace the
    /// pending row, mirroring `HistoryState::replace_id`.
    pub(crate) fn replace_id(&mut self, old: MessageId, new_message: HistoryMessage) {
        self.messages.remove(&old.0);
        self.upsert(new_message);
    }
}

/// In-chat search (tdesktop `searchInChat` / ComposeSearch): `searchChatMessages`.
#[derive(Debug, Clone)]
pub struct ChatSearchState {
    pub open: bool,
    pub chat_id: Option<ChatId>,
    pub query: String,
    pub generation: u64,
    pub status: SearchStatus,
    pub hits: Vec<SearchMessageHit>,
    pub total_count: i32,
    pub next_from_message_id: MessageId,
    pub selected: Option<usize>,
    pub jump: ChatSearchJump,
    /// Bumped by every jump request, so the UI can restart its highlight
    /// fade when the same message is jumped to again.
    pub jump_serial: u64,
}

impl Default for ChatSearchState {
    fn default() -> Self {
        Self {
            open: false,
            chat_id: None,
            query: String::new(),
            generation: 0,
            status: SearchStatus::Closed,
            hits: Vec::new(),
            total_count: 0,
            next_from_message_id: MessageId(0),
            selected: None,
            jump: ChatSearchJump::None,
            jump_serial: 0,
        }
    }
}

impl ChatSearchState {
    pub fn open_for(&mut self, chat_id: ChatId) {
        if self.open && self.chat_id == Some(chat_id) {
            return;
        }
        self.open = true;
        self.chat_id = Some(chat_id);
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Idle;
        self.clear_results();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.chat_id = None;
        self.query.clear();
        self.status = SearchStatus::Closed;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.open = true;
        self.query = query.to_string();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.generation
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
        self.status = if self.open {
            SearchStatus::Idle
        } else {
            SearchStatus::Closed
        };
    }

    pub(crate) fn clear_results(&mut self) {
        self.hits.clear();
        self.total_count = 0;
        self.next_from_message_id = MessageId(0);
        self.selected = None;
        self.jump = ChatSearchJump::None;
    }

    pub(crate) fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.open
            && pending.and_then(|p| p.chat_id) == self.chat_id
    }

    pub(crate) fn accept_hits(
        &mut self,
        hits: Vec<SearchMessageHit>,
        total_count: i32,
        next_from_message_id: MessageId,
        error: bool,
    ) {
        self.hits = hits;
        self.total_count = total_count;
        self.next_from_message_id = next_from_message_id;
        self.selected = if self.hits.is_empty() { None } else { Some(0) };
        self.jump = ChatSearchJump::None;
        self.status = if !self.hits.is_empty() {
            SearchStatus::Ready
        } else if error {
            SearchStatus::Failed
        } else {
            SearchStatus::Empty
        };
    }

    pub fn selected_hit(&self) -> Option<&SearchMessageHit> {
        self.selected.and_then(|i| self.hits.get(i))
    }

    /// Newer hit (Unigram `NextExecute`: lower index; results are newest-first).
    pub fn select_newer(&mut self) -> Option<MessageId> {
        let index = self.selected?;
        if index == 0 {
            return None;
        }
        self.selected = Some(index - 1);
        self.hits.get(index - 1).map(|hit| hit.message_id)
    }

    /// Older hit (Unigram `PreviousExecute`: higher index).
    pub fn select_older(&mut self) -> Option<MessageId> {
        let index = self.selected?;
        if index + 1 >= self.hits.len() {
            return None;
        }
        self.selected = Some(index + 1);
        self.hits.get(index + 1).map(|hit| hit.message_id)
    }

    pub fn select_message(&mut self, message_id: MessageId) -> bool {
        let Some(index) = self
            .hits
            .iter()
            .position(|hit| hit.message_id == message_id)
        else {
            return false;
        };
        self.selected = Some(index);
        true
    }

    pub fn position_label(&self) -> String {
        match (self.selected, self.hits.len()) {
            (Some(i), n) if n > 0 => format!("{} of {n}", i + 1),
            _ => String::new(),
        }
    }
}
