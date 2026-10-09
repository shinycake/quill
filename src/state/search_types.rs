//! Search state types: global, in-chat and topic search.
use super::*;

/// Pending inline confirmations of the search panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchConfirm {
    /// `lng_recent_clear_sure`: clear the whole search history.
    ClearRecents,
    /// `lng_recent_hide_sure`: clear and disable the frequent contacts.
    DisableTopChats,
}

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
    pub extras: MessageExtras,
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
            extras: message.extras.clone(),
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
            extras: self.extras,
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
    /// The filter bar under the field: chat type, media tab, date window.
    pub filters: crate::search_filters::GlobalSearchFilters,
    /// `searchChatsOnServer` results, merged behind the offline
    /// `searchChats` hits (see [`Self::merged_chat_ids`]). They never gate
    /// the status: the server answer is a supplement, not a requirement.
    pub server_chat_ids: Vec<ChatId>,
    /// "Frequent contacts" (`getTopChats` users), shown on an empty search.
    pub top_chats: Vec<ChatId>,
    /// TDLib option `disable_top_chats` (tdesktop "Suggest frequent
    /// contacts" off): the strip is hidden and nothing is fetched.
    pub top_chats_disabled: bool,
    /// An inline confirmation row is showing (tdesktop asks before clearing
    /// the history or disabling the frequent contacts).
    pub confirm: Option<SearchConfirm>,
    /// The frequent contact whose "Remove from Recent" row is open.
    pub top_menu: Option<ChatId>,
    /// `searchPublicPosts` said the free daily quota is spent.
    pub public_limits_exceeded: bool,
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
            filters: crate::search_filters::GlobalSearchFilters::default(),
            server_chat_ids: Vec::new(),
            top_chats: Vec::new(),
            top_chats_disabled: false,
            confirm: None,
            top_menu: None,
            public_limits_exceeded: false,
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
        self.filters = Default::default();
        self.confirm = None;
        self.top_menu = None;
        self.status = SearchStatus::Closed;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.recents = true;
        self.community_filter = None;
        self.filters = Default::default();
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
        self.server_chat_ids.clear();
        self.public_limits_exceeded = false;
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

    /// The chats section: the offline hits first, then what only the server
    /// knew (no duplicates).
    pub fn merged_chat_ids(&self) -> Vec<ChatId> {
        let mut merged = self.chat_ids.clone();
        for id in &self.server_chat_ids {
            if !merged.contains(id) {
                merged.push(*id);
            }
        }
        merged
    }

    /// The public-chats section without anything already in the chats one.
    pub fn public_only_chat_ids(&self) -> Vec<ChatId> {
        let merged = self.merged_chat_ids();
        self.public_chat_ids
            .iter()
            .copied()
            .filter(|id| !merged.contains(id))
            .collect()
    }

    /// `searchChatsOnServer` answer: late hits can turn "no results" into
    /// results, never the other way round.
    pub(crate) fn accept_server_chats(&mut self, chat_ids: Vec<ChatId>) {
        self.server_chat_ids = chat_ids;
        if self.chats_done && self.messages_done && self.public_done {
            self.finish_if_complete();
        }
    }

    /// Public-posts scope: only one request is in flight; the chat sections
    /// are not searched.
    pub fn begin_public_scope(&mut self) {
        self.chats_done = true;
        self.public_done = true;
    }

    /// Drop one entry of the Recent list (`removeRecentlyFoundChat`).
    pub fn remove_recent(&mut self, chat_id: ChatId) -> bool {
        if !self.recents {
            return false;
        }
        let before = self.chat_ids.len();
        self.chat_ids.retain(|id| *id != chat_id);
        let removed = self.chat_ids.len() != before;
        if removed && self.chat_ids.is_empty() && self.messages.is_empty() {
            self.status = SearchStatus::Idle;
        }
        removed
    }

    /// Drop one frequent contact (`removeTopChat`).
    pub fn remove_top_chat(&mut self, chat_id: ChatId) -> bool {
        let before = self.top_chats.len();
        self.top_chats.retain(|id| *id != chat_id);
        if self.top_menu == Some(chat_id) {
            self.top_menu = None;
        }
        self.top_chats.len() != before
    }

    pub(crate) fn finish_if_complete(&mut self) {
        if !(self.chats_done && self.messages_done && self.public_done) {
            return;
        }
        let any = !self.chat_ids.is_empty()
            || !self.server_chat_ids.is_empty()
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

/// Which unread marker a corner jump button walks (tdesktop
/// `CornerButtonType::Mentions` / `Reactions` / `PollVotes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadJumpKind {
    Mention,
    Reaction,
    /// B15: votes in the user's polls (`searchMessagesFilterUnreadPollVote`,
    /// `readAllChatPollVotes`).
    PollVote,
}

impl UnreadJumpKind {
    /// `searchMessagesFilter*` constructor (`schema/td_api.tl:6317,6320,6323`).
    pub fn filter_constructor(self) -> &'static str {
        match self {
            Self::Mention => "searchMessagesFilterUnreadMention",
            Self::Reaction => "searchMessagesFilterUnreadReaction",
            Self::PollVote => "searchMessagesFilterUnreadPollVote",
        }
    }
}

/// The oldest of a `searchChatMessages` page (TDLib answers newest first,
/// tdesktop jumps to the oldest unread marker first).
pub fn oldest_message_id(ids: impl IntoIterator<Item = MessageId>) -> Option<MessageId> {
    ids.into_iter().filter(|id| id.0 > 0).min_by_key(|id| id.0)
}

/// The history's "N Unread Messages" bar text (tdesktop
/// `lng_unread_bar#one` / `#other`); a chat opened without a known count
/// keeps a plain label.
pub fn unread_bar_text(count: i32) -> String {
    match count {
        i32::MIN..=0 => "Unread Messages".into(),
        1 => "1 Unread Message".into(),
        n => format!("{n} Unread Messages"),
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
    /// Subsection tabs: `foundChatMessages.total_count` of the latest
    /// page — the topic header's "N messages".
    pub total_count: i32,
}

impl Default for TopicHistory {
    fn default() -> Self {
        Self {
            messages: BTreeMap::new(),
            next_from_message_id: MessageId(0),
            loaded_complete: false,
            total_count: 0,
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
    /// "From: member" (`sender_id` of `searchChatMessages`).
    pub sender: Option<MessageSender>,
    /// The media tab (`filter` of `searchChatMessages`).
    pub media: crate::search_filters::SearchMediaKind,
    /// The member picker replacing the results while choosing a sender.
    pub from_picker: Option<FromPicker>,
    /// An older page is in flight.
    pub loading_more: bool,
}

/// The "From:" picker: group members filtered by the field's text
/// (tdesktop `dialogs_search_from_controllers`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FromPicker {
    /// The text the members were last searched with.
    pub query: String,
    pub members: Vec<MessageSender>,
    /// The in-flight `searchChatMembers`; other answers are dropped.
    pub request: Option<RequestId>,
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
            sender: None,
            media: crate::search_filters::SearchMediaKind::All,
            from_picker: None,
            loading_more: false,
        }
    }
}

impl ChatSearchState {
    /// A search runs for a non-empty query, a chosen sender or a media tab.
    pub fn has_criteria(&self) -> bool {
        !self.query.is_empty()
            || self.sender.is_some()
            || self.media != crate::search_filters::SearchMediaKind::All
    }

    /// An older page can be fetched (the results are a partial first page).
    pub fn can_load_more(&self) -> bool {
        self.open
            && !self.loading_more
            && self.next_from_message_id.0 != 0
            && (self.hits.len() as i32) < self.total_count
    }

    /// Append an older page (deduped; `selected` stays on its message).
    pub(crate) fn append_hits(
        &mut self,
        hits: Vec<SearchMessageHit>,
        total_count: i32,
        next_from_message_id: MessageId,
    ) {
        self.loading_more = false;
        for hit in hits {
            if !self.hits.iter().any(|h| h.message_id == hit.message_id) {
                self.hits.push(hit);
            }
        }
        self.total_count = total_count.max(self.hits.len() as i32);
        self.next_from_message_id = next_from_message_id;
    }

    pub fn open_for(&mut self, chat_id: ChatId) {
        if self.open && self.chat_id == Some(chat_id) {
            return;
        }
        self.open = true;
        self.chat_id = Some(chat_id);
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Idle;
        self.sender = None;
        self.media = crate::search_filters::SearchMediaKind::All;
        self.from_picker = None;
        self.clear_results();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.chat_id = None;
        self.query.clear();
        self.sender = None;
        self.media = crate::search_filters::SearchMediaKind::All;
        self.from_picker = None;
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
        self.loading_more = false;
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

    /// tdesktop `lng_search_messages_n_of_amount`: "N of M" with M the
    /// server's total, not just the pages loaded so far.
    pub fn position_label(&self) -> String {
        match (self.selected, self.hits.len()) {
            (Some(i), n) if n > 0 => {
                format!("{} of {}", i + 1, (self.total_count.max(0) as usize).max(n))
            }
            _ => String::new(),
        }
    }
}
