//! Connect driver: search, shared media, sponsored messages.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId, TopicId};
use crate::state::{ChatSearchJumpNeed, RequestPurpose, SearchStatus, SharedMediaTab};
use crate::telegram::envelope::ChatKind;
use crate::telegram::requests::{
    SendReply, add_recently_found_chat, click_chat_sponsored_message, get_chat_history,
    get_chat_sponsored_messages, report_chat_sponsored_message, search_chat_messages, search_chats,
    search_messages, search_messages_filter_json, search_public_chats, search_recently_found_chats,
    view_sponsored_chat,
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
        if let Err(err) = self.sender.send_json(&search_messages(
            messages_extra,
            trimmed,
            SEARCH_LIMIT,
            self.session.search.community_filter,
        )) {
            self.abort_typed_search(chats_extra, messages_extra, public_extra);
            return Err(err);
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

    fn request_recents(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
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
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &TopicId::None,
            trimmed,
            MessageId(0),
            0,
            CHAT_SEARCH_LIMIT,
            None,
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
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        match self.session.begin_chat_search_jump(message_id) {
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
