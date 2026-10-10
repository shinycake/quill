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

impl<S: JsonSender> ConnectDriver<S> {
    /// `getChatSponsoredMessages` for a channel chat (TDLib 1.8.67). Called
    /// when a channel is opened; rows render Sponsored / Recommended.
    /// The fetch already runs so the pipeline is proven with replay
    /// fixtures. Bot chats can also carry sponsored messages per the
    /// schema; they are not fetched yet (Phase 3).
    pub fn fetch_sponsored_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_channel = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
        if !is_channel {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatSponsoredMessages, chat_id)
            || !self
                .session
                .sponsored_fetch_due(chat_id, std::time::Instant::now())
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatSponsoredMessages, Some(chat_id));
        match self
            .sender
            .send_json(&get_chat_sponsored_messages(extra, chat_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Tell TDLib the sponsored messages `shown` are on screen
    /// (`viewMessages`, `messageSourceChatHistory`; TDLib 1.8.67 has no
    /// separate `viewSponsoredMessage`). The session counts each id once, so
    /// calling this every frame the ad is visible sends at most one request
    /// per ad. Returns the request id when something was sent.
    pub fn view_sponsored_messages(
        &mut self,
        chat_id: ChatId,
        shown: &[i64],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let due = self.session.take_sponsored_views(chat_id, shown);
        if due.is_empty() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewSponsoredMessages, Some(chat_id));
        let ids: Vec<MessageId> = due.iter().copied().map(MessageId).collect();
        match self.sender.send_json(&view_messages(
            extra,
            chat_id,
            &ids,
            "messageSourceChatHistory",
            false,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.untake_sponsored_views(chat_id, &due);
                Err(err)
            }
        }
    }

    /// "Hide ads" (tdesktop `HideSponsoredClickHandler`): Premium accounts
    /// send `toggleHasSponsoredMessagesEnabled(false)`; for anyone else the
    /// session records the "needs Premium" notice and nothing is sent
    /// (`Ok(None)`).
    pub fn hide_sponsored_messages(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.begin_sponsored_hide(chat_id, message_id) != Some(true) {
            return Ok(None);
        }
        let extra = self.session.request(
            RequestPurpose::ToggleHasSponsoredMessagesEnabled,
            Some(chat_id),
        );
        match self
            .sender
            .send_json(&toggle_has_sponsored_messages_enabled(extra, false))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `reportChatSponsoredMessage` (TDLib 1.8.67). Empty `option_id` starts
    /// the flow; TDLib may answer `reportSponsoredResultOptionRequired`.
    /// Returns `None` when the row is missing or `can_be_reported` is false.
    pub fn report_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        option_id: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .begin_sponsored_report(chat_id, message_id)
            .is_none()
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReportChatSponsoredMessage, Some(chat_id));
        match self.sender.send_json(&report_chat_sponsored_message(
            extra, chat_id, message_id, option_id,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.dismiss_sponsored_report();
                Err(err)
            }
        }
    }

    /// `clickChatSponsoredMessage` (TDLib 1.8.67): the user opened a sponsored
    /// message's sponsor link/button (`is_media_click = false`) or its media
    /// (`is_media_click = true`). Fire-and-forget; the `ok` response is ignored.
    pub fn click_chat_sponsored_message(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
        is_media_click: bool,
        from_fullscreen: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ClickChatSponsoredMessage, Some(chat_id));
        match self.sender.send_json(&click_chat_sponsored_message(
            extra,
            chat_id,
            message_id,
            is_media_click,
            from_fullscreen,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `viewSponsoredChat` (TDLib 1.8.67): the user fully viewed a sponsored
    /// chat. The unique id comes from `sponsoredChat` search results.
    pub fn view_sponsored_chat(
        &mut self,
        sponsored_chat_unique_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ViewSponsoredChat, None);
        match self
            .sender
            .send_json(&view_sponsored_chat(extra, sponsored_chat_unique_id))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

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

    /// tdesktop `searchInChat` when history is focused (`Command::Search` / Ctrl+F).
    pub fn open_chat_search(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(self.session.open_chat_search())
    }

    pub fn close_chat_search(&mut self) {
        self.clear_chat_search_debounce();
        self.session.close_chat_search();
    }

    /// Slice media-shared-gallery: open the gallery for the open chat and
    /// fetch the active tab's first page. Returns `false` when there is no
    /// open chat to gallery-ize.
    pub fn open_shared_media(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(false);
        };
        // Reopening the gallery for the same chat keeps the already-fetched
        // tabs (`SharedMediaState::open_for`); only a fresh open fetches the
        // active tab.
        let reopening =
            self.session.shared_media.open && self.session.shared_media.chat_id == Some(chat_id);
        let tab = self.session.shared_media.open_for(chat_id);
        if !reopening {
            self.fetch_shared_media(tab)?;
        }
        Ok(true)
    }

    pub fn close_shared_media(&mut self) {
        self.session.shared_media.close();
    }

    /// Slice media-shared-gallery: switch tabs; fetch only tabs that were
    /// never fetched (each tab caches its first page).
    pub fn select_shared_media_tab(&mut self, tab: SharedMediaTab) -> Result<(), ConnectSendError> {
        if self.session.shared_media.select_tab(tab) {
            self.fetch_shared_media(tab)?;
        }
        Ok(())
    }

    /// Slice media-shared-gallery: one `searchChatMessages` page with the
    /// tab's `searchMessagesFilter*` filter (`schema/td_api.tl:11864`).
    pub fn fetch_shared_media(&mut self, tab: SharedMediaTab) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.shared_media.chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let generation = self.session.shared_media.begin_fetch(tab);
        let extra = self.session.request(
            RequestPurpose::GetSharedMedia { tab, generation },
            Some(chat_id),
        );
        let filter = search_messages_filter_json(tab.filter_constructor());
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &TopicId::None,
            "",
            MessageId(0),
            0,
            SHARED_MEDIA_PAGE_SIZE,
            Some(filter),
        )) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.shared_media.fail(
                    chat_id,
                    tab,
                    generation,
                    "Could not send the shared-media request.".to_string(),
                );
                Err(err)
            }
        }
    }

    /// The media viewer paged toward the end of a gallery tab: fetch the
    /// next older `searchChatMessages` page (tdesktop loads more of
    /// `SharedMediaWithLastSlice` as the viewer nears an edge). A no-op
    /// when the list is complete or a page is already in flight.
    pub fn fetch_more_shared_media(&mut self, tab: SharedMediaTab) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.shared_media.chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let Some((generation, from_message_id)) = self.session.shared_media.begin_fetch_more(tab)
        else {
            return Ok(());
        };
        let extra = self.session.request(
            RequestPurpose::Media(MediaPurpose::GetSharedMediaMore { tab, generation }),
            Some(chat_id),
        );
        let filter = search_messages_filter_json(tab.filter_constructor());
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &TopicId::None,
            "",
            from_message_id,
            0,
            SHARED_MEDIA_PAGE_SIZE,
            Some(filter),
        )) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.session.requests.take(extra);
                self.session
                    .shared_media
                    .fail_more(chat_id, tab, generation);
                Err(err)
            }
        }
    }

    /// Slice media-shared-gallery: gallery row click — close the gallery and
    /// jump to the message with the same history-around pipeline in-chat
    /// search jumps use (`jump_to_replied_message` does the same).
    pub fn jump_to_shared_media_item(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.shared_media.close();
        self.jump_to_chat_search_message(message_id)
    }

    /// Empty query: clear immediately (tdesktop ComposeSearch skips empty).
    /// Non-empty: debounce `AutoSearchTimeout` (900 ms), then `searchChatMessages`.
    pub fn set_chat_search_query(
        &mut self,
        query: &str,
    ) -> Result<ChatSearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_search.open && !self.session.open_chat_search() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() && self.session.chat_search.has_criteria() {
            // A chosen sender or media tab keeps the search going without
            // text (tdesktop lists the member's messages on their own).
            self.clear_chat_search_debounce();
            let state = &self.session.chat_search;
            if state.query.is_empty()
                && matches!(
                    state.status,
                    SearchStatus::Searching
                        | SearchStatus::Ready
                        | SearchStatus::Empty
                        | SearchStatus::Failed
                )
            {
                return Ok(ChatSearchQueryOutcome::Unchanged);
            }
            return self.restart_chat_search("");
        }
        if trimmed.is_empty() {
            self.clear_chat_search_debounce();
            if self.session.chat_search.query.is_empty()
                && matches!(
                    self.session.chat_search.status,
                    SearchStatus::Idle | SearchStatus::Closed
                )
            {
                return Ok(ChatSearchQueryOutcome::Unchanged);
            }
            self.session.chat_search.clear_query();
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        if self
            .pending_typed_chat_search
            .as_ref()
            .is_some_and(|(_, q)| q == trimmed)
            && self.session.chat_search.query == trimmed
        {
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        if self.pending_typed_chat_search.is_none()
            && self.session.chat_search.query == trimmed
            && matches!(
                self.session.chat_search.status,
                SearchStatus::Searching
                    | SearchStatus::Ready
                    | SearchStatus::Empty
                    | SearchStatus::Failed
            )
        {
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        let _search_gen = self.session.chat_search.begin_query(trimmed);
        self.chat_search_debounce_token = self.chat_search_debounce_token.saturating_add(1);
        let token = self.chat_search_debounce_token;
        self.pending_typed_chat_search = Some((token, trimmed.to_string()));
        Ok(ChatSearchQueryOutcome::Debounced { token })
    }

    /// Re-run the search at once with the current sender / media criteria.
    fn restart_chat_search(
        &mut self,
        query: &str,
    ) -> Result<ChatSearchQueryOutcome, ConnectSendError> {
        self.clear_chat_search_debounce();
        let _ = self.session.chat_search.begin_query(query);
        Ok(match self.send_chat_search(query)? {
            Some(flight) => ChatSearchQueryOutcome::Sent(flight),
            None => ChatSearchQueryOutcome::Unchanged,
        })
    }

    /// Choose (or clear with `None`) the "From:" member; the search reruns.
    pub fn set_chat_search_sender(
        &mut self,
        sender: Option<MessageSender>,
    ) -> Result<ChatSearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_search.open {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chat_search.sender = sender;
        self.session.close_from_picker();
        let query = self.session.chat_search.query.clone();
        if query.is_empty() && !self.session.chat_search.has_criteria() {
            self.clear_chat_search_debounce();
            self.session.chat_search.clear_query();
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        self.restart_chat_search(&query)
    }

    /// Pick the media tab of the in-chat search; the search reruns.
    pub fn set_chat_search_media(
        &mut self,
        media: SearchMediaKind,
    ) -> Result<ChatSearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_search.open {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chat_search.media = media;
        let query = self.session.chat_search.query.clone();
        if query.is_empty() && !self.session.chat_search.has_criteria() {
            self.clear_chat_search_debounce();
            self.session.chat_search.clear_query();
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        self.restart_chat_search(&query)
    }

    /// Open the "From:" picker and list the group's members.
    pub fn open_chat_search_from_picker(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_search_can_pick_sender() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.open_from_picker();
        self.search_from_members("")
    }

    /// `searchChatMembers` for the picker's field text (empty = everyone).
    pub fn search_from_members(
        &mut self,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.chat_search.chat_id.or(self.session.open_chat) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let Some(picker) = self.session.chat_search.from_picker.as_ref() else {
            return Ok(None);
        };
        if picker.query == query && (picker.request.is_some() || !picker.members.is_empty()) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::SearchFromMembers, Some(chat_id));
        if let Some(picker) = self.session.chat_search.from_picker.as_mut() {
            picker.query = query.to_string();
            picker.request = Some(extra);
        }
        match self.sender.send_json(&search_chat_members(
            extra,
            chat_id,
            query,
            FROM_MEMBERS_LIMIT,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                if let Some(picker) = self.session.chat_search.from_picker.as_mut() {
                    picker.request = None;
                }
                Err(err)
            }
        }
    }

    /// Fetch the next older page of hits (the first page holds
    /// [`CHAT_SEARCH_LIMIT`]; "N of M" counts the server's total).
    pub fn load_more_chat_search(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_search.can_load_more() {
            return Ok(None);
        }
        let Some(chat_id) = self.session.chat_search.chat_id.or(self.session.open_chat) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let state = &self.session.chat_search;
        let (search_gen, from, query, sender, media) = (
            state.generation,
            state.next_from_message_id,
            state.query.clone(),
            state.sender,
            state.media,
        );
        let extra = self.session.request_chat_search(
            RequestPurpose::SearchChatMessagesMore,
            chat_id,
            search_gen,
        );
        self.session.chat_search.loading_more = true;
        match self.sender.send_json(&search_chat_messages_from(
            extra,
            chat_id,
            &TopicId::None,
            &query,
            sender,
            from,
            0,
            CHAT_SEARCH_LIMIT,
            media.constructor().map(search_messages_filter_json),
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.chat_search.loading_more = false;
                Err(err)
            }
        }
    }

    /// Open the calendar box on the current month (media tabs highlight
    /// their days).
    pub fn open_history_calendar(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let now = crate::local_time::now_unix();
        if self.session.open_history_calendar(now).is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.fetch_calendar_page()
    }

    pub fn show_calendar_month(&mut self, month: YearMonth) -> Result<(), ConnectSendError> {
        if let Some(calendar) = self.session.history_calendar.as_mut() {
            calendar.show_month(month);
        }
        self.fetch_calendar_page()
    }

    /// Page `getChatMessageCalendar` back until the displayed month is covered.
    pub fn fetch_calendar_page(&mut self) -> Result<(), ConnectSendError> {
        let Some(calendar) = self.session.history_calendar.as_ref() else {
            return Ok(());
        };
        if !calendar.needs_older_page() {
            return Ok(());
        }
        let Some(constructor) = calendar.media.constructor() else {
            return Ok(());
        };
        let (chat_id, generation, from) = (
            calendar.chat_id,
            calendar.generation,
            calendar.oldest_loaded,
        );
        let extra = self.session.request(
            RequestPurpose::Search(SearchPurpose::GetChatMessageCalendar { generation }),
            Some(chat_id),
        );
        if let Some(calendar) = self.session.history_calendar.as_mut() {
            calendar.loading = true;
        }
        match self.sender.send_json(&get_chat_message_calendar(
            extra,
            chat_id,
            constructor,
            from,
        )) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.session.requests.take(extra);
                if let Some(calendar) = self.session.history_calendar.as_mut() {
                    calendar.fail();
                }
                Err(err)
            }
        }
    }

    pub fn close_history_calendar(&mut self) {
        self.session.close_history_calendar();
    }

    /// A day was picked: a calendar day goes straight to its first message,
    /// any other asks TDLib for the last message before the day and lands
    /// on the one after it (`getChatMessageByDate`).
    pub fn jump_to_date(&mut self, day_number: i64) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(calendar) = self.session.history_calendar.take() else {
            return Ok(None);
        };
        self.session.date_jump_note = None;
        if let Some(day) = calendar.day(day_number) {
            return self.jump_to_message_with(day.message_id, DateJumpMode::Exact);
        }
        let chat_id = calendar.chat_id;
        let extra = self
            .session
            .request(RequestPurpose::GetChatMessageByDate, Some(chat_id));
        match self.sender.send_json(&get_chat_message_by_date(
            extra,
            chat_id,
            crate::search_filters::before_day_date(day_number),
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.fail_date_jump(false);
                Err(err)
            }
        }
    }

    pub fn commit_debounced_chat_search(
        &mut self,
        token: u64,
    ) -> Result<Option<ChatSearchFlight>, ConnectSendError> {
        let Some((pending_token, query)) = self.pending_typed_chat_search.clone() else {
            return Ok(None);
        };
        if pending_token != token {
            return Ok(None);
        }
        self.pending_typed_chat_search = None;
        self.send_chat_search(&query)
    }

    fn clear_chat_search_debounce(&mut self) {
        self.pending_typed_chat_search = None;
        self.chat_search_debounce_token = self.chat_search_debounce_token.saturating_add(1);
    }

    fn send_chat_search(
        &mut self,
        trimmed: &str,
    ) -> Result<Option<ChatSearchFlight>, ConnectSendError> {
        let Some(chat_id) = self.session.chat_search.chat_id.or(self.session.open_chat) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let search_gen = self.session.chat_search.generation;
        let extra = self.session.request_chat_search(
            RequestPurpose::SearchChatMessages,
            chat_id,
            search_gen,
        );
        let (sender, media) = (
            self.session.chat_search.sender,
            self.session.chat_search.media,
        );
        match self.sender.send_json(&search_chat_messages_from(
            extra,
            chat_id,
            &TopicId::None,
            trimmed,
            sender,
            MessageId(0),
            0,
            CHAT_SEARCH_LIMIT,
            media.constructor().map(search_messages_filter_json),
        )) {
            Ok(()) => Ok(Some(ChatSearchFlight::Query(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session
                    .chat_search
                    .accept_hits(Vec::new(), 0, MessageId(0), true);
                Err(err)
            }
        }
    }

    pub fn jump_to_chat_search_message(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.jump_to_message_with(message_id, DateJumpMode::Exact)
    }

    /// The shared jump pipeline; `mode` retargets a date jump once the
    /// window around `message_id` is loaded.
    pub(crate) fn jump_to_message_with(
        &mut self,
        message_id: MessageId,
        mode: DateJumpMode,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        match self.session.begin_date_jump(message_id, mode) {
            ChatSearchJumpNeed::AlreadyReady | ChatSearchJumpNeed::Missing => Ok(None),
            ChatSearchJumpNeed::LoadAround => {
                // The target is outside the loaded window: replace the
                // window instead of dropping a disconnected slice into it.
                if let Some(chat_id) = self.session.chat_search.chat_id.or(self.session.open_chat) {
                    self.session.reset_history_window(chat_id);
                }
                self.fetch_history_around(message_id)
            }
        }
    }

    /// Quote-strip activation: same Unigram `LoadMessageSliceImpl` around-load
    /// + highlight pipeline as in-chat search jump (`getChatHistory` offset -25).
    pub fn jump_to_replied_message(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.jump_to_chat_search_message(message_id)
    }

    pub fn jump_selected_chat_search_hit(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self
            .session
            .chat_search
            .selected_hit()
            .map(|hit| hit.message_id)
        else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    pub fn chat_search_newer(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self.session.chat_search.select_newer() else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    pub fn chat_search_older(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        // Prefetch the next page before the last loaded hit is reached.
        let state = &self.session.chat_search;
        if state
            .selected
            .is_some_and(|i| i + CHAT_SEARCH_PREFETCH >= state.hits.len())
        {
            let _ = self.load_more_chat_search();
        }
        let Some(message_id) = self.session.chat_search.select_older() else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    /// Unigram `GetChatHistory(chatId, maxId, -25, 50)` around the jump target.
    fn fetch_history_around(
        &mut self,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self.session.request_history_around(chat_id, message_id);
        match self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            message_id,
            HISTORY_AROUND_OFFSET,
            HISTORY_AROUND_LIMIT,
            false,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.chat_search.jump =
                    crate::state::ChatSearchJump::Missing { message_id };
                Err(err)
            }
        }
    }
}
