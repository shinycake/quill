//! Search state types: global, in-chat and topic search.
use super::*;

/// Pending inline confirmations of the search panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchConfirm {
    /// `lng_recent_clear_sure`: clear the whole search history.
    ClearRecents,
    /// `lng_recent_hide_sure`: clear and disable the frequent contacts.
    DisableTopChats,
    /// Spend Stars on one public-posts search once the free ones are gone.
    /// The query and the price are the ones the box showed: accepting
    /// spends exactly that, or nothing when either has changed since.
    PaidPostSearch { query: String, price: i64 },
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
            send: crate::state::SendState::Sent,
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
            ephemeral: None,
        }
    }
}

/// The surface the global search panel shows. `Closed` leaves when the
/// field opens (`Recents`) or a query starts (`Query`); `Recents` and `Query`
/// leave on `close`, and a typed query clears back to `Recents`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchSurface {
    #[default]
    Closed,
    /// The empty field: `searchRecentlyFoundChats`.
    Recents,
    /// The results of a query, a filter or a browsed media tab.
    Query,
}

/// One request of a global search (chats, messages or public chats).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Leg {
    /// Nothing asked yet (the field is open, or the results were reset).
    #[default]
    Idle,
    /// This search does not make the request.
    Skipped,
    /// Asked, no answer yet.
    Pending,
    /// The paid public-posts search, confirmed for `price` Stars, is on its
    /// way as `request`. Only its answer (or its failure) leaves it.
    Paying {
        request: RequestId,
        price: i64,
    },
    Done,
    /// A public-posts search answered, and the free searches are used up:
    /// another one costs Stars. Cleared with the results.
    Exceeded,
    /// The request answered with an error.
    Failed,
}

impl Leg {
    fn answered(error: bool) -> Self {
        if error { Self::Failed } else { Self::Done }
    }

    /// Asked, no answer yet.
    fn waiting(self) -> bool {
        matches!(self, Self::Pending | Self::Paying { .. })
    }
}

/// `status` is derived (see [`SearchState::status`]): the search is open or
/// not, and what is known about its three requests and its results is all the
/// UI needs.
#[derive(Debug, Clone)]
pub struct SearchState {
    /// Which surface the global search shows ([`SearchSurface`]): closed,
    /// the empty-field Recent list, or the results of a query.
    pub surface: SearchSurface,
    pub query: String,
    pub generation: u64,
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
    /// The top-people strip: collapsed to one row or expanded to a grid.
    pub top_view: crate::top_people::StripView,
    /// Paging, recommendations and quota of the strip's tabs.
    pub tabs: SearchTabsState,
    /// The offline `searchChats` request.
    pub(crate) chats: Leg,
    /// `searchMessages`.
    pub(crate) messages_leg: Leg,
    /// `searchPublicChats`.
    pub(crate) public: Leg,
    /// The typed query waiting out its quiet window. A new generation
    /// (close, clear, filters, a query started) makes it stale, so it is
    /// dropped there and cannot outlive the search it belongs to.
    pub(crate) typed: super::debounce::Debounce<String>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            surface: SearchSurface::Closed,
            query: String::new(),
            generation: 0,
            chat_ids: Vec::new(),
            messages: Vec::new(),
            public_chat_ids: Vec::new(),
            community_filter: None,
            filters: crate::search_filters::GlobalSearchFilters::default(),
            server_chat_ids: Vec::new(),
            top_chats: Vec::new(),
            top_chats_disabled: false,
            confirm: None,
            top_view: Default::default(),
            tabs: SearchTabsState::default(),
            chats: Leg::Idle,
            messages_leg: Leg::Idle,
            public: Leg::Idle,
            typed: Default::default(),
        }
    }
}

impl SearchState {
    /// A new generation: the answers asked before it are dropped, and a
    /// typed query still waiting its quiet window with them.
    fn bump_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.typed.cancel();
    }

    /// The global search is open.
    pub fn is_open(&self) -> bool {
        self.surface != SearchSurface::Closed
    }

    /// The empty-query surface: `searchRecentlyFoundChats` (official
    /// Recent).
    pub fn is_recents(&self) -> bool {
        self.surface == SearchSurface::Recents
    }

    pub fn open_field(&mut self) {
        if self.is_open() {
            return;
        }
        self.surface = SearchSurface::Recents;
        self.query.clear();
        self.clear_results();
    }

    pub fn close(&mut self) {
        self.surface = SearchSurface::Closed;
        self.query.clear();
        self.community_filter = None;
        self.filters = Default::default();
        self.confirm = None;
        self.top_view = Default::default();
        self.bump_generation();
        self.clear_results();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        if self.is_open() {
            self.surface = SearchSurface::Recents;
        }
        self.community_filter = None;
        self.filters = Default::default();
        self.bump_generation();
        self.clear_results();
    }

    /// What the search panel shows, from what is open, which requests are
    /// out or answered, and whether anything was found.
    pub fn status(&self) -> SearchStatus {
        if !self.is_open() {
            return SearchStatus::Closed;
        }
        let legs = [self.chats, self.messages_leg, self.public];
        if legs.iter().any(|leg| leg.waiting()) {
            return SearchStatus::Searching;
        }
        let any = !self.chat_ids.is_empty()
            || !self.server_chat_ids.is_empty()
            || !self.messages.is_empty()
            || !self.public_chat_ids.is_empty();
        if any {
            SearchStatus::Ready
        } else if legs.contains(&Leg::Idle) {
            SearchStatus::Idle
        } else if legs.contains(&Leg::Failed) {
            SearchStatus::Failed
        } else if self.is_recents() {
            SearchStatus::Idle
        } else {
            SearchStatus::Empty
        }
    }

    /// Empty search field: wait only for `searchRecentlyFoundChats`.
    pub fn begin_recents(&mut self) -> u64 {
        self.surface = SearchSurface::Recents;
        self.query.clear();
        self.bump_generation();
        self.clear_results();
        self.chats = Leg::Pending;
        self.messages_leg = Leg::Skipped;
        self.public = Leg::Skipped;
        self.generation
    }

    /// A media tab that ignores the typed text (Photos, Videos, Voice):
    /// the query stays in the field's mirror, the list restarts.
    pub fn begin_browse(&mut self) -> u64 {
        self.surface = SearchSurface::Query;
        self.bump_generation();
        self.clear_results();
        self.mark_all_pending();
        self.generation
    }

    /// The filter bar changed: what was asked under the old tab is no
    /// longer wanted, and its answers are dropped by the new generation.
    pub fn restart_for_filters(&mut self) {
        self.bump_generation();
        self.clear_results();
        if self.is_recents() {
            self.surface = SearchSurface::Query;
        }
    }

    fn mark_all_pending(&mut self) {
        self.chats = Leg::Pending;
        self.messages_leg = Leg::Pending;
        self.public = Leg::Pending;
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.surface = SearchSurface::Query;
        self.query = query.to_string();
        self.bump_generation();
        self.clear_results();
        self.mark_all_pending();
        self.generation
    }

    pub(crate) fn clear_results(&mut self) {
        self.chat_ids.clear();
        self.messages.clear();
        self.public_chat_ids.clear();
        self.server_chat_ids.clear();
        self.tabs.reset_paging();
        self.chats = Leg::Idle;
        self.messages_leg = Leg::Idle;
        self.public = Leg::Idle;
        // A paid-search question was asked about these results' query: it
        // goes with them, rather than stay over a list it no longer fits.
        if matches!(self.confirm, Some(SearchConfirm::PaidPostSearch { .. })) {
            self.confirm = None;
        }
    }

    pub(crate) fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.is_open()
    }

    pub fn accept_chats(&mut self, chat_ids: Vec<ChatId>, error: bool) {
        self.chat_ids = chat_ids;
        self.chats = Leg::answered(error);
    }

    pub fn accept_messages(&mut self, messages: Vec<SearchMessageHit>, error: bool) {
        self.messages = messages;
        self.messages_leg = Leg::answered(error);
    }

    /// The answer of a public-posts search said whether the free searches
    /// are used up. Only a leg that answered carries it.
    pub fn note_public_limits(&mut self, exceeded: bool) {
        if matches!(self.messages_leg, Leg::Done | Leg::Exceeded) {
            self.messages_leg = if exceeded { Leg::Exceeded } else { Leg::Done };
        }
    }

    /// The paid search goes out as `request` for `price` Stars; it replaces
    /// the posts leg the free attempt left.
    pub fn begin_paid_posts(&mut self, request: RequestId, price: i64) {
        self.messages_leg = Leg::Paying { request, price };
    }

    /// The paid search in flight and the price it was confirmed for.
    pub fn paying(&self) -> Option<(RequestId, i64)> {
        match self.messages_leg {
            Leg::Paying { request, price } => Some((request, price)),
            _ => None,
        }
    }

    /// The free public-post searches are used up for the open query.
    pub fn public_limits_exceeded(&self) -> bool {
        self.messages_leg == Leg::Exceeded
    }

    pub fn accept_public_chats(&mut self, chat_ids: Vec<ChatId>, error: bool) {
        self.public_chat_ids = chat_ids;
        self.public = Leg::answered(error);
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
    }

    /// Public-posts scope: only one request is in flight; the chat sections
    /// are not searched.
    pub fn begin_public_scope(&mut self) {
        self.chats = Leg::Skipped;
        self.public = Leg::Skipped;
    }

    /// Drop one entry of the Recent list (`removeRecentlyFoundChat`).
    pub fn remove_recent(&mut self, chat_id: ChatId) -> bool {
        if !self.is_recents() {
            return false;
        }
        let before = self.chat_ids.len();
        self.chat_ids.retain(|id| *id != chat_id);
        let removed = self.chat_ids.len() != before;
        if removed && self.chat_ids.is_empty() && self.messages.is_empty() {
            self.chats = Leg::Idle;
        }
        removed
    }

    /// The Recent list was cleared (`clearRecentlyFoundChats`): back to the
    /// empty-field surface.
    pub fn clear_recents(&mut self) {
        self.chat_ids.clear();
        self.chats = Leg::Idle;
    }

    /// Put a recent chat back where `remove_recent` took it from (the
    /// server refused the removal), unless the list has moved on.
    pub fn restore_recent(&mut self, chat_id: ChatId, index: usize) {
        if !self.is_recents() || self.chat_ids.contains(&chat_id) {
            return;
        }
        self.chat_ids
            .insert(index.min(self.chat_ids.len()), chat_id);
        if self.chats == Leg::Idle {
            self.chats = Leg::Done;
        }
    }

    /// Put a frequent contact back (the server refused `removeTopChat`).
    pub fn restore_top_chat(&mut self, chat_id: ChatId, index: usize) {
        if self.top_chats_disabled || self.top_chats.contains(&chat_id) {
            return;
        }
        self.top_chats
            .insert(index.min(self.top_chats.len()), chat_id);
    }

    /// Drop one frequent contact (`removeTopChat`).
    pub fn remove_top_chat(&mut self, chat_id: ChatId) -> bool {
        let before = self.top_chats.len();
        self.top_chats.retain(|id| *id != chat_id);
        self.top_chats.len() != before
    }
}

/// What a load-around jump does once its window is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpLanding {
    /// Land on the message itself.
    Message,
    /// A date jump in `chat_id`: land where `mode` says relative to the
    /// message TDLib resolved.
    Date { chat_id: ChatId, mode: DateJumpMode },
}

/// Jump-to-message after an in-chat hit (Unigram `LoadMessageSliceAsync`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchJump {
    None,
    /// `getChatHistory` around the target is in flight — not yet loaded.
    /// `then` is what the loaded window does next: land on the message, or
    /// retarget a date jump (the jump leaves with this stage).
    Loading {
        message_id: MessageId,
        then: JumpLanding,
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
            Self::Loading { message_id, .. }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchJumpNeed {
    AlreadyReady,
    Missing,
    LoadAround,
}

/// How the older end of a thread or topic window stands after its last page
/// request. A failed end waits for a jump to the latest or a reopen instead of
/// asking again on every render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OlderEnd {
    #[default]
    Open,
    /// A Saved Messages page request is out (sublist or tag filter): only
    /// its answer extends the list, and an answer to any other (a view left
    /// and entered again) is dropped. Leaves with the answer, its error, or
    /// the window being replaced.
    Asking(RequestId),
    Failed,
    /// A page reached the start of the topic (`next_from_message_id` 0, or
    /// an empty page): nothing older to ask for. Leaves when the oldest
    /// end is trimmed away or the window is replaced.
    Complete,
}

/// The newer end of a topic window, the twin of [`OlderEnd`]. `More`: the
/// window stops short of the newest message, newer pages load as the
/// reader scrolls down; `Failed`: the last newer page failed and
/// auto-paging stops until a jump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewerEnd {
    /// The window reaches the newest message.
    #[default]
    AtLatest,
    More,
    Failed,
}

/// In-chat search (tdesktop `searchInChat` / ComposeSearch): `searchChatMessages`.
/// What survives closing the bar (the generation that drops late answers,
/// the jump a hit starts, the quiet window of a typed query) lives here;
/// everything the open bar owns is replaced whole with its surface.
#[derive(Debug, Clone)]
pub struct ChatSearchState {
    pub surface: ChatSearchSurface,
    pub generation: u64,
    pub jump: ChatSearchJump,
    /// Bumped by every jump request, so the UI can restart its highlight
    /// fade when the same message is jumped to again.
    pub jump_serial: u64,
    /// The typed query waiting out its quiet window; a new generation
    /// drops it.
    pub(crate) typed: super::debounce::Debounce<String>,
}

/// The in-chat search bar: shut, or open on one chat with everything it
/// holds. Opening and closing replace the value, so nothing of a search
/// outlives its bar.
#[derive(Debug, Clone, Default)]
pub enum ChatSearchSurface {
    #[default]
    Closed,
    Open(OpenChatSearch),
}

/// Where the open bar stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatSearchPhase {
    #[default]
    Idle,
    Searching,
    Ready,
    Empty,
    Failed,
}

impl From<ChatSearchPhase> for SearchStatus {
    fn from(phase: ChatSearchPhase) -> Self {
        match phase {
            ChatSearchPhase::Idle => Self::Idle,
            ChatSearchPhase::Searching => Self::Searching,
            ChatSearchPhase::Ready => Self::Ready,
            ChatSearchPhase::Empty => Self::Empty,
            ChatSearchPhase::Failed => Self::Failed,
        }
    }
}

/// An open in-chat search.
#[derive(Debug, Clone)]
pub struct OpenChatSearch {
    pub chat_id: ChatId,
    pub query: String,
    pub phase: ChatSearchPhase,
    pub hits: Vec<SearchMessageHit>,
    pub total_count: i32,
    /// The older pages: where they continue from, and whether one is in
    /// flight or failed.
    pub paging: ChatSearchPaging,
    pub selected: Option<usize>,
    /// "From: member" (`sender_id` of `searchChatMessages`).
    pub sender: Option<MessageSender>,
    /// The media tab (`filter` of `searchChatMessages`).
    pub media: crate::search_filters::SearchMediaKind,
    /// The member picker replacing the results while choosing a sender.
    pub from_picker: Option<FromPicker>,
}

impl OpenChatSearch {
    fn new(chat_id: ChatId) -> Self {
        Self {
            chat_id,
            query: String::new(),
            phase: ChatSearchPhase::Idle,
            hits: Vec::new(),
            total_count: 0,
            paging: ChatSearchPaging::End,
            selected: None,
            sender: None,
            media: crate::search_filters::SearchMediaKind::All,
            from_picker: None,
        }
    }

    fn clear_results(&mut self) {
        self.hits.clear();
        self.total_count = 0;
        self.paging = ChatSearchPaging::End;
        self.selected = None;
    }
}

/// The older pages of an in-chat search (`foundChatMessages` pages by
/// `next_from_message_id`, 0 = no more). A failed page waits for a Retry
/// instead of ending the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatSearchPaging {
    /// No older page (the server sent cursor 0, or nothing is loaded).
    #[default]
    End,
    /// An older page can be asked from this cursor.
    More { from: MessageId },
    /// The page from this cursor is in flight.
    Loading { from: MessageId },
    /// The page from this cursor failed; asking again is the user's Retry.
    Failed { from: MessageId },
}

impl ChatSearchPaging {
    /// The page after the server's answer: its cursor, 0 ending the list.
    fn after(next_from_message_id: MessageId) -> Self {
        if next_from_message_id.0 == 0 {
            Self::End
        } else {
            Self::More {
                from: next_from_message_id,
            }
        }
    }
}

/// The "From:" picker: group members filtered by the field's text
/// (tdesktop `dialogs_search_from_controllers`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FromPicker {
    /// The text the members were last searched with.
    pub query: String,
    /// The members for `query`: the in-flight `searchChatMembers` holds its
    /// request (other answers are dropped) and the previous list meanwhile.
    pub members: crate::state::fetch::Fetch<Vec<MessageSender>>,
}

impl FromPicker {
    /// The members to list: the answer, or the last one while a new
    /// query loads or after it failed.
    pub fn members(&self) -> &[MessageSender] {
        self.members.data().map_or(&[], Vec::as_slice)
    }

    /// A member search is out.
    pub fn loading(&self) -> bool {
        self.members.is_loading()
    }
}

impl Default for ChatSearchState {
    fn default() -> Self {
        Self {
            surface: ChatSearchSurface::Closed,
            generation: 0,
            jump: ChatSearchJump::None,
            jump_serial: 0,
            typed: Default::default(),
        }
    }
}

impl ChatSearchState {
    /// A new generation: the answers asked before it are dropped, and a
    /// typed query still waiting its quiet window with them.
    fn bump_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.typed.cancel();
    }

    /// The open search, when the bar is open.
    pub fn open(&self) -> Option<&OpenChatSearch> {
        match &self.surface {
            ChatSearchSurface::Open(open) => Some(open),
            ChatSearchSurface::Closed => None,
        }
    }

    pub fn open_mut(&mut self) -> Option<&mut OpenChatSearch> {
        match &mut self.surface {
            ChatSearchSurface::Open(open) => Some(open),
            ChatSearchSurface::Closed => None,
        }
    }

    /// The in-chat search bar is open.
    pub fn is_open(&self) -> bool {
        self.open().is_some()
    }

    /// The chat the open bar searches.
    pub fn chat_id(&self) -> Option<ChatId> {
        self.open().map(|open| open.chat_id)
    }

    pub fn query(&self) -> &str {
        self.open().map_or("", |open| open.query.as_str())
    }

    /// What the bar shows (`Closed` when it is not open).
    pub fn status(&self) -> SearchStatus {
        self.open()
            .map_or(SearchStatus::Closed, |open| open.phase.into())
    }

    pub fn hits(&self) -> &[SearchMessageHit] {
        self.open().map_or(&[], |open| open.hits.as_slice())
    }

    pub fn total_count(&self) -> i32 {
        self.open().map_or(0, |open| open.total_count)
    }

    pub fn selected(&self) -> Option<usize> {
        self.open().and_then(|open| open.selected)
    }

    pub fn sender(&self) -> Option<MessageSender> {
        self.open().and_then(|open| open.sender)
    }

    pub fn media(&self) -> crate::search_filters::SearchMediaKind {
        self.open()
            .map_or(crate::search_filters::SearchMediaKind::All, |open| {
                open.media
            })
    }

    pub fn from_picker(&self) -> Option<&FromPicker> {
        self.open().and_then(|open| open.from_picker.as_ref())
    }

    /// Choose (or clear) the "From:" member of the open bar.
    pub fn set_sender(&mut self, sender: Option<MessageSender>) {
        if let Some(open) = self.open_mut() {
            open.sender = sender;
        }
    }

    /// Choose the media tab of the open bar.
    pub fn set_media(&mut self, media: crate::search_filters::SearchMediaKind) {
        if let Some(open) = self.open_mut() {
            open.media = media;
        }
    }

    /// Open or close the member picker of the open bar.
    pub fn set_from_picker(&mut self, picker: Option<FromPicker>) {
        if let Some(open) = self.open_mut() {
            open.from_picker = picker;
        }
    }

    /// A search runs for a non-empty query, a chosen sender or a media tab.
    pub fn has_criteria(&self) -> bool {
        self.open().is_some_and(|open| {
            !open.query.is_empty()
                || open.sender.is_some()
                || open.media != crate::search_filters::SearchMediaKind::All
        })
    }

    /// An older page can be fetched (the results are a partial first page).
    pub fn can_load_more(&self) -> bool {
        self.open().is_some_and(|open| {
            matches!(open.paging, ChatSearchPaging::More { .. })
                && (open.hits.len() as i32) < open.total_count
        })
    }

    /// The cursor of the page to ask: the next one, or the failed one the
    /// user retries.
    pub fn page_to_ask(&self) -> Option<MessageId> {
        match self.open()?.paging {
            ChatSearchPaging::More { from } if self.can_load_more() => Some(from),
            ChatSearchPaging::Failed { from } => Some(from),
            _ => None,
        }
    }

    /// The page from `from` goes out.
    pub(crate) fn begin_more(&mut self, from: MessageId) {
        if let Some(open) = self.open_mut() {
            open.paging = ChatSearchPaging::Loading { from };
        }
    }

    /// The page that was going out never left, or failed: back to waiting
    /// for the user's Retry.
    pub(crate) fn fail_more(&mut self) {
        if let Some(open) = self.open_mut()
            && let ChatSearchPaging::Loading { from } = open.paging
        {
            open.paging = ChatSearchPaging::Failed { from };
        }
    }

    /// An older page is in flight.
    pub fn loading_more(&self) -> bool {
        self.open()
            .is_some_and(|open| matches!(open.paging, ChatSearchPaging::Loading { .. }))
    }

    /// The last older page failed (the Retry strip shows).
    pub fn more_failed(&self) -> bool {
        self.open()
            .is_some_and(|open| matches!(open.paging, ChatSearchPaging::Failed { .. }))
    }

    /// Append an older page (deduped; `selected` stays on its message).
    pub(crate) fn append_hits(
        &mut self,
        hits: Vec<SearchMessageHit>,
        total_count: i32,
        next_from_message_id: MessageId,
    ) {
        let Some(open) = self.open_mut() else {
            return;
        };
        for hit in hits {
            if !open.hits.iter().any(|h| h.message_id == hit.message_id) {
                open.hits.push(hit);
            }
        }
        open.total_count = total_count.max(open.hits.len() as i32);
        open.paging = ChatSearchPaging::after(next_from_message_id);
    }

    /// Open the bar on `chat_id`: a fresh search, unless it already is.
    pub fn open_for(&mut self, chat_id: ChatId) {
        if self.chat_id() == Some(chat_id) {
            return;
        }
        self.surface = ChatSearchSurface::Open(OpenChatSearch::new(chat_id));
        self.bump_generation();
        self.jump = ChatSearchJump::None;
    }

    pub fn close(&mut self) {
        self.surface = ChatSearchSurface::Closed;
        self.bump_generation();
        self.jump = ChatSearchJump::None;
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.bump_generation();
        self.jump = ChatSearchJump::None;
        if let Some(open) = self.open_mut() {
            open.query = query.to_string();
            open.phase = ChatSearchPhase::Searching;
            open.clear_results();
        }
        self.generation
    }

    pub fn clear_query(&mut self) {
        self.bump_generation();
        self.jump = ChatSearchJump::None;
        if let Some(open) = self.open_mut() {
            open.query.clear();
            open.phase = ChatSearchPhase::Idle;
            open.clear_results();
        }
    }

    pub(crate) fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.is_open()
            && pending.and_then(|p| p.chat_id) == self.chat_id()
    }

    pub(crate) fn accept_hits(
        &mut self,
        hits: Vec<SearchMessageHit>,
        total_count: i32,
        next_from_message_id: MessageId,
        error: bool,
    ) {
        self.jump = ChatSearchJump::None;
        let Some(open) = self.open_mut() else {
            return;
        };
        open.hits = hits;
        open.total_count = total_count;
        open.paging = ChatSearchPaging::after(next_from_message_id);
        open.selected = if open.hits.is_empty() { None } else { Some(0) };
        open.phase = if !open.hits.is_empty() {
            ChatSearchPhase::Ready
        } else if error {
            ChatSearchPhase::Failed
        } else {
            ChatSearchPhase::Empty
        };
    }

    pub fn selected_hit(&self) -> Option<&SearchMessageHit> {
        let open = self.open()?;
        open.selected.and_then(|i| open.hits.get(i))
    }

    /// Newer hit (Unigram `NextExecute`: lower index; results are newest-first).
    pub fn select_newer(&mut self) -> Option<MessageId> {
        let open = self.open_mut()?;
        let index = open.selected?;
        if index == 0 {
            return None;
        }
        open.selected = Some(index - 1);
        open.hits.get(index - 1).map(|hit| hit.message_id)
    }

    /// Older hit (Unigram `PreviousExecute`: higher index).
    pub fn select_older(&mut self) -> Option<MessageId> {
        let open = self.open_mut()?;
        let index = open.selected?;
        if index + 1 >= open.hits.len() {
            return None;
        }
        open.selected = Some(index + 1);
        open.hits.get(index + 1).map(|hit| hit.message_id)
    }

    pub fn select_message(&mut self, message_id: MessageId) -> bool {
        let Some(open) = self.open_mut() else {
            return false;
        };
        let Some(index) = open.hits.iter().position(|hit| hit.message_id == message_id) else {
            return false;
        };
        open.selected = Some(index);
        true
    }

    /// tdesktop `lng_search_messages_n_of_amount`: "N of M" with M the
    /// server's total, not just the pages loaded so far.
    pub fn position_label(&self) -> String {
        match (self.selected(), self.hits().len()) {
            (Some(i), n) if n > 0 => {
                format!("{} of {}", i + 1, (self.total_count().max(0) as usize).max(n))
            }
            _ => String::new(),
        }
    }
}
