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
    /// The query is typed and its request is not sent yet (the typing
    /// debounce holds it). Leaves when the request goes out
    /// ([`SearchState::leg_sent`]), the debounce is cancelled, or the
    /// results are reset.
    Queued,
    /// Asked as `request`; only its answer or failure leaves the stage.
    Pending {
        request: RequestId,
    },
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
        matches!(
            self,
            Self::Queued | Self::Pending { .. } | Self::Paying { .. }
        )
    }

    /// The request this leg waits for.
    fn request(self) -> Option<RequestId> {
        match self {
            Self::Pending { request } | Self::Paying { request, .. } => Some(request),
            _ => None,
        }
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
    /// Bumped when a new query, tab or filter starts the list over: the UI
    /// scrolls to the top. It does not match answers; every request is
    /// matched by the `RequestId` its stage holds.
    pub epoch: u64,
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
    pub server_chats: crate::state::fetch::Fetch<Vec<ChatId>>,
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
    /// The typed query waiting out its quiet window. A new list
    /// (close, clear, filters, a query started) makes it stale, so it is
    /// dropped there and cannot outlive the search it belongs to.
    pub(crate) typed: super::debounce::Debounce<String>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            surface: SearchSurface::Closed,
            query: String::new(),
            epoch: 0,
            chat_ids: Vec::new(),
            messages: Vec::new(),
            public_chat_ids: Vec::new(),
            community_filter: None,
            filters: crate::search_filters::GlobalSearchFilters::default(),
            server_chats: Default::default(),
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
    /// A new list: the answers asked before it find no stage that waits for
    /// them (the legs are reset by the caller), and a typed query still
    /// waiting its quiet window goes with them.
    fn bump_epoch(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
        self.typed.cancel();
    }

    /// The typed query's quiet window is cancelled: what waited for it
    /// is not asked after all.
    pub fn cancel_typed(&mut self) {
        self.typed.cancel();
        for leg in [&mut self.chats, &mut self.messages_leg, &mut self.public] {
            if *leg == Leg::Queued {
                *leg = Leg::Idle;
            }
        }
    }

    /// The request for `purpose` went out as `request`: the leg it belongs
    /// to waits for it. A request outside the legs (the supplement, the
    /// quota, a page) is held by its own stage.
    pub(crate) fn leg_sent(&mut self, purpose: RequestPurpose, request: RequestId) {
        let leg = match purpose {
            RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats => {
                &mut self.chats
            }
            RequestPurpose::SearchMessages
            | RequestPurpose::SearchPublicPosts
            | RequestPurpose::SearchPublicMessagesByTag => &mut self.messages_leg,
            RequestPurpose::SearchPublicChats => &mut self.public,
            RequestPurpose::SearchChatsOnServer => {
                self.server_chats.begin();
                self.server_chats.sent(request);
                return;
            }
            _ => return,
        };
        *leg = Leg::Pending { request };
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
        self.bump_epoch();
        self.clear_results();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        if self.is_open() {
            self.surface = SearchSurface::Recents;
        }
        self.community_filter = None;
        self.filters = Default::default();
        self.bump_epoch();
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
            || !self.server_chats.items().is_empty()
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
    pub fn begin_recents(&mut self) {
        self.surface = SearchSurface::Recents;
        self.query.clear();
        self.bump_epoch();
        self.clear_results();
        self.chats = Leg::Queued;
        self.messages_leg = Leg::Skipped;
        self.public = Leg::Skipped;
    }

    /// A media tab that ignores the typed text (Photos, Videos, Voice):
    /// the query stays in the field's mirror, the list restarts.
    pub fn begin_browse(&mut self) {
        self.surface = SearchSurface::Query;
        self.bump_epoch();
        self.clear_results();
        self.mark_all_queued();
    }

    /// The filter bar changed: what was asked under the old tab is no
    /// longer wanted, and its answers find no stage that waits for them.
    pub fn restart_for_filters(&mut self) {
        self.bump_epoch();
        self.clear_results();
        if self.is_recents() {
            self.surface = SearchSurface::Query;
        }
    }

    fn mark_all_queued(&mut self) {
        self.chats = Leg::Queued;
        self.messages_leg = Leg::Queued;
        self.public = Leg::Queued;
    }

    pub fn begin_query(&mut self, query: &str) {
        self.surface = SearchSurface::Query;
        self.query = query.to_string();
        self.bump_epoch();
        self.clear_results();
        self.mark_all_queued();
    }

    pub(crate) fn clear_results(&mut self) {
        self.chat_ids.clear();
        self.messages.clear();
        self.public_chat_ids.clear();
        self.server_chats = Default::default();
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

    /// The answer of `pending` is the one a stage of the open search waits
    /// for: a leg of its purpose holding this request, or the page now out.
    /// An older query's request finds its stage reset or holding another id.
    pub(crate) fn awaits(&self, pending: Option<&PendingRequest>) -> bool {
        let Some(pending) = pending else {
            return false;
        };
        if !self.is_open() {
            return false;
        }
        let leg = match pending.purpose {
            RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats => self.chats,
            RequestPurpose::SearchMessages
            | RequestPurpose::SearchPublicPosts
            | RequestPurpose::SearchPublicMessagesByTag => self.messages_leg,
            RequestPurpose::SearchPublicChats => self.public,
            RequestPurpose::SearchChatsOnServer => {
                return self.server_chats.waits_for(pending.id);
            }
            RequestPurpose::GetPublicPostSearchLimits => {
                return self.tabs.limits.waits_for(pending.id);
            }
            RequestPurpose::SearchMessagesMore | RequestPurpose::SearchPublicPostsMore => {
                return self.tabs.paging
                    == (SearchPaging::Loading {
                        request: pending.id,
                    });
            }
            _ => return false,
        };
        leg.request() == Some(pending.id)
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
        for id in self.server_chats.items() {
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
    pub(crate) fn accept_server_chats(&mut self, request: RequestId, chat_ids: Vec<ChatId>) {
        self.server_chats.answered(Some(request), chat_ids);
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
