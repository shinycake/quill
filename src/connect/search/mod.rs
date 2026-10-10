//! Connect driver: search, shared media, sponsored messages.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId, TopicId};
use crate::search_filters::{SearchMediaKind, YearMonth};
use crate::state::DateJumpMode;
use crate::state::{ChatSearchJumpNeed, RequestPurpose, SearchStatus, SharedMediaTab};
use crate::state::{MediaPurpose, SearchPurpose};
use crate::telegram::envelope::ChatKind;
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests::{
    SendReply, add_recently_found_chat, click_chat_sponsored_message, get_chat_history,
    get_chat_message_by_date, get_chat_message_calendar, get_chat_sponsored_messages,
    get_top_chats_users, remove_recently_found_chat, remove_top_chat_users,
    report_chat_sponsored_message, search_chat_members, search_chat_messages,
    search_chat_messages_from, search_chats, search_chats_on_server, search_messages_filter_json,
    search_messages_filtered, search_public_chats, search_public_messages_by_tag,
    search_public_posts, search_recently_found_chats, set_option_boolean,
    toggle_has_sponsored_messages_enabled, view_messages, view_sponsored_chat,
};

mod chat_search;
mod sponsored;

impl<S: JsonSender> ConnectDriver<S> {
    pub fn open_search(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.clear_typed_debounce();
        if self.session.search.open && !self.session.search.query.is_empty() {
            return Ok(None);
        }
        if self.session.search.open
            && self.session.search.recents
            && matches!(
                self.session.search.status,
                SearchStatus::Searching | SearchStatus::Ready | SearchStatus::Idle
            )
        {
            return Ok(None);
        }
        self.request_recents()
    }

    pub fn close_search(&mut self) {
        self.clear_typed_debounce();
        self.session.close_search();
    }

    /// Empty query: `searchRecentlyFoundChats` immediately (official Recent).
    /// Non-empty: debounce, then `searchChats` + `searchMessages` (`chat_list` null).
    pub fn set_search_query(
        &mut self,
        query: &str,
    ) -> Result<SearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.clear_typed_debounce();
            if self.session.search.open
                && self.session.search.query.is_empty()
                && self.session.search.recents
                && matches!(
                    self.session.search.status,
                    SearchStatus::Searching | SearchStatus::Ready | SearchStatus::Idle
                )
            {
                return Ok(SearchQueryOutcome::Unchanged);
            }
            return Ok(match self.request_recents()? {
                Some(flight) => SearchQueryOutcome::Sent(flight),
                None => SearchQueryOutcome::Unchanged,
            });
        }
        if self
            .pending_typed_search
            .as_ref()
            .is_some_and(|(_, q)| q == trimmed)
            && self.session.search.query == trimmed
        {
            return Ok(SearchQueryOutcome::Unchanged);
        }
        if self.pending_typed_search.is_none()
            && self.session.search.query == trimmed
            && !self.session.search.recents
            && matches!(
                self.session.search.status,
                SearchStatus::Searching
                    | SearchStatus::Ready
                    | SearchStatus::Empty
                    | SearchStatus::Failed
            )
        {
            return Ok(SearchQueryOutcome::Unchanged);
        }
        let _search_gen = self.session.search.begin_query(trimmed);
        self.search_debounce_token = self.search_debounce_token.saturating_add(1);
        let token = self.search_debounce_token;
        self.pending_typed_search = Some((token, trimmed.to_string()));
        Ok(SearchQueryOutcome::Debounced { token })
    }

    /// Send the settled typed query if `token` is still the latest debounce.
    pub fn commit_debounced_search(
        &mut self,
        token: u64,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        let Some((pending_token, query)) = self.pending_typed_search.clone() else {
            return Ok(None);
        };
        if pending_token != token {
            return Ok(None);
        }
        self.pending_typed_search = None;
        self.send_typed_search(&query)
    }

    fn clear_typed_debounce(&mut self) {
        self.pending_typed_search = None;
        self.search_debounce_token = self.search_debounce_token.saturating_add(1);
    }

    /// Slice (communities-search-filter): community filter chip in the
    /// typed-search panel. Stores the selection in
    /// `SearchState::community_filter` and immediately re-runs the current
    /// query so `searchMessages` carries `searchMessagesChatTypeFilterCommunity`
    /// (schema 1.8.67, line 6344); `None` (the "All chats" chip) restores the
    /// null filter. With no query text the selection is just stored for the
    /// next search.
    pub fn set_search_community_filter(
        &mut self,
        community_id: Option<i64>,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.clear_typed_debounce();
        self.session.search.community_filter = community_id;
        let query = self.session.search.query.clone();
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let _search_gen = self.session.search.begin_query(trimmed);
        self.send_typed_search(trimmed)
    }

    /// The global-search filter bar changed (chat type, media tab, date
    /// window): store it and rerun the current query so `searchMessages`
    /// carries the new `chat_type_filter` / `filter` / `min_date`.
    pub fn set_search_filters(
        &mut self,
        filters: crate::search_filters::GlobalSearchFilters,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.clear_typed_debounce();
        self.session.search.filters = filters;
        let query = self.session.search.query.clone();
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let _search_gen = self.session.search.begin_query(trimmed);
        self.send_typed_search(trimmed)
    }

    /// Correlate a failed typed search: drop the pending requests and mark
    /// all three searches errored so the status resolves instead of
    /// stranding the query in `Searching`.
    fn abort_typed_search(
        &mut self,
        chats_extra: RequestId,
        messages_extra: RequestId,
        public_extra: RequestId,
    ) {
        self.session.requests.take(chats_extra);
        self.session.requests.take(messages_extra);
        self.session.requests.take(public_extra);
        self.session.search.accept_chats(Vec::new(), true);
        self.session.search.accept_messages(Vec::new(), true);
        self.session.search.accept_public_chats(Vec::new(), true);
    }

    /// Typed query: `searchChats` + `searchPublicChats` + `searchMessages`
    /// (Phase 7.2 adds the public username lookup alongside the offline
    /// known-chat search).
    fn send_typed_search(
        &mut self,
        trimmed: &str,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        let search_gen = self.session.search.generation;
        if self.session.search.filters.scope == crate::search_filters::SearchScope::PublicPosts {
            return self.send_public_posts_search(trimmed, search_gen);
        }
        // The Apps tab lists bots the session already knows; nothing to
        // search for, so the query resolves at once.
        if !self.session.search.filters.scope.searches_messages() {
            self.session.search.accept_chats(Vec::new(), false);
            self.session.search.accept_messages(Vec::new(), false);
            self.session.search.accept_public_chats(Vec::new(), false);
            return Ok(None);
        }
        let chats_extra = self
            .session
            .request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = self
            .session
            .request_search(RequestPurpose::SearchMessages, search_gen);
        let public_extra = self
            .session
            .request_search(RequestPurpose::SearchPublicChats, search_gen);
        if let Err(err) = self
            .sender
            .send_json(&search_chats(chats_extra, trimmed, SEARCH_LIMIT))
        {
            self.abort_typed_search(chats_extra, messages_extra, public_extra);
            return Err(err);
        }
        let filters = crate::telegram::requests::SearchMessagesFilters {
            community_id: self.session.search.community_filter,
            chat_type: self.session.search.filters.chat_type,
            media: self.session.search.filters.media,
            min_date: self
                .session
                .search
                .filters
                .date
                .min_date(crate::local_time::now_unix()),
            max_date: self
                .session
                .search
                .filters
                .date
                .max_date(crate::local_time::now_unix()),
            archived: self.session.search.filters.archived,
        };
        if let Err(err) = self.sender.send_json(&search_messages_filtered(
            messages_extra,
            trimmed,
            SEARCH_LIMIT,
            &filters,
        )) {
            self.abort_typed_search(chats_extra, messages_extra, public_extra);
            return Err(err);
        }
        // The server's own title/username search: a supplement merged behind
        // the offline hits, fire-and-forget on failure.
        let server_extra = self
            .session
            .request_search(RequestPurpose::SearchChatsOnServer, search_gen);
        if self
            .sender
            .send_json(&search_chats_on_server(server_extra, trimmed, SEARCH_LIMIT))
            .is_err()
        {
            self.session.requests.take(server_extra);
        }
        match self
            .sender
            .send_json(&search_public_chats(public_extra, trimmed))
        {
            Ok(()) => Ok(Some(SearchFlight::Query(
                chats_extra,
                messages_extra,
                public_extra,
            ))),
            Err(err) => {
                self.abort_typed_search(chats_extra, messages_extra, public_extra);
                Err(err)
            }
        }
    }

    /// The "Public posts" scope: a hashtag or cashtag goes to
    /// `searchPublicMessagesByTag`, anything else to `searchPublicPosts`
    /// (free searches only). The chat sections are not searched.
    fn send_public_posts_search(
        &mut self,
        trimmed: &str,
        search_gen: u64,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        self.session.search.begin_public_scope();
        let tag = crate::search_filters::tag_query(trimmed);
        let purpose = if tag.is_some() {
            RequestPurpose::SearchPublicMessagesByTag
        } else {
            RequestPurpose::SearchPublicPosts
        };
        let extra = self.session.request_search(purpose, search_gen);
        let json = match tag {
            Some(tag) => search_public_messages_by_tag(extra, tag, "", SEARCH_LIMIT),
            None => search_public_posts(extra, trimmed, "", SEARCH_LIMIT),
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(SearchFlight::PublicPosts(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.search.accept_messages(Vec::new(), true);
                Err(err)
            }
        }
    }

    /// Hashtag click (tdesktop `searchByHashtag`): search `tag` in the
    /// chosen scope — "My messages" or "Public posts" (the third scope,
    /// "This chat", is the in-chat search and lives in the UI).
    pub fn search_hashtag(
        &mut self,
        tag: &str,
        scope: crate::search_filters::SearchScope,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.clear_typed_debounce();
        self.session.search.open = true;
        self.session.search.filters.scope = scope;
        let _search_gen = self.session.search.begin_query(tag);
        self.send_typed_search(tag)
    }

    /// `removeRecentlyFoundChat`: one entry leaves the Recent list at once
    /// (the schema has no update for it; a refusal surfaces as a note).
    pub fn remove_recent_search(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.search.remove_recent(chat_id) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::RemoveRecentlyFoundChat, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&remove_recently_found_chat(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `getTopChats(users)` for the "Frequent contacts" strip; skipped while
    /// the strip is disabled.
    pub fn fetch_top_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.search.top_chats_disabled
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetTopChats)
        {
            return Ok(None);
        }
        let extra = self.session.request(RequestPurpose::GetTopChats, None);
        if let Err(err) = self
            .sender
            .send_json(&get_top_chats_users(extra, TOP_CHATS_LIMIT))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// "Remove from Recent" on a frequent contact (`removeTopChat`).
    pub fn remove_top_chat(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.search.remove_top_chat(chat_id) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::RemoveTopChat, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&remove_top_chat_users(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Settings > "Suggest frequent contacts" (and tdesktop's "Remove all &
    /// Disable"): `setOption(disable_top_chats)`. Turning it off clears the
    /// strip at once; `updateOption` confirms the truth.
    pub fn set_top_chats_disabled(
        &mut self,
        disabled: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetTopChatsDisabled, None);
        if let Err(err) =
            self.sender
                .send_json(&set_option_boolean(extra, "disable_top_chats", disabled))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.search.top_chats_disabled = disabled;
        if disabled {
            self.session.search.top_chats.clear();
            self.session.search.top_menu = None;
        }
        Ok(extra)
    }

    fn request_recents(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
        // The frequent contacts ride along; their failure never blocks the
        // Recent list.
        let _ = self.fetch_top_chats();
        let search_gen = self.session.search.begin_recents();
        let extra = self
            .session
            .request_search(RequestPurpose::SearchRecentlyFoundChats, search_gen);
        match self
            .sender
            .send_json(&search_recently_found_chats(extra, "", RECENT_SEARCH_LIMIT))
        {
            Ok(()) => Ok(Some(SearchFlight::Recents(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.search.accept_chats(Vec::new(), true);
                Err(err)
            }
        }
    }

    fn remember_found_chat(&mut self, chat_id: ChatId) {
        let extra = self
            .session
            .request(RequestPurpose::AddRecentlyFoundChat, Some(chat_id));
        if self
            .sender
            .send_json(&add_recently_found_chat(extra, chat_id))
            .is_err()
        {
            self.session.requests.take(extra);
        }
    }

    /// Open a chat from search via `addRecentlyFoundChat` then `openChat`.
    /// Flushes the leaving composer's draft before `select_chat` drops `pending_draft`.
    pub fn select_search_chat(
        &mut self,
        chat_id: ChatId,
        leaving_text: &str,
        leaving_reply: Option<SendReply>,
        now_ms: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.flush_leaving_composer(chat_id, leaving_text, leaving_reply, now_ms)?;
        self.remember_found_chat(chat_id);
        self.session.close_search();
        self.select_chat(chat_id)
    }

    /// Jump to a found message: upsert it into history, then `select_chat`.
    /// Same flush-before-drop as [`Self::select_search_chat`].
    pub fn select_search_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        leaving_text: &str,
        leaving_reply: Option<SendReply>,
        now_ms: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.flush_leaving_composer(chat_id, leaving_text, leaving_reply, now_ms)?;
        self.remember_found_chat(chat_id);
        self.session.close_search();
        let opened = self.select_chat(chat_id)?;
        // Open the found message in context (a window around it, with the
        // highlight) rather than splicing the lone hit into the history.
        match self.jump_to_chat_search_message(message_id)? {
            Some(extra) => Ok(Some(extra)),
            None => Ok(opened),
        }
    }
}
