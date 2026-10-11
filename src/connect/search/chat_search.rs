//! Connect driver: shared media, in-chat search, the history calendar and message jumps.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
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
        let reopening = self.session.media.shared_media.open
            && self.session.media.shared_media.chat_id == Some(chat_id);
        let tab = self.session.media.shared_media.open_for(chat_id);
        if !reopening {
            self.fetch_shared_media(tab)?;
        }
        Ok(true)
    }

    pub fn close_shared_media(&mut self) {
        self.session.media.shared_media.close();
    }

    /// Slice media-shared-gallery: switch tabs; fetch only tabs that were
    /// never fetched (each tab caches its first page).
    pub fn select_shared_media_tab(&mut self, tab: SharedMediaTab) -> Result<(), ConnectSendError> {
        if self.session.media.shared_media.select_tab(tab) {
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
        let Some(chat_id) = self.session.media.shared_media.chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let generation = self.session.media.shared_media.begin_fetch(tab);
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
                self.session.media.shared_media.fail(
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
        let Some(chat_id) = self.session.media.shared_media.chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let Some((generation, from_message_id)) =
            self.session.media.shared_media.begin_fetch_more(tab)
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
                    .media
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
        self.session.media.shared_media.close();
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
        if !self.session.search.chat_search.open && !self.session.open_chat_search() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() && self.session.search.chat_search.has_criteria() {
            // A chosen sender or media tab keeps the search going without
            // text (tdesktop lists the member's messages on their own).
            self.clear_chat_search_debounce();
            let state = &self.session.search.chat_search;
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
            if self.session.search.chat_search.query.is_empty()
                && matches!(
                    self.session.search.chat_search.status,
                    SearchStatus::Idle | SearchStatus::Closed
                )
            {
                return Ok(ChatSearchQueryOutcome::Unchanged);
            }
            self.session.search.chat_search.clear_query();
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        if self
            .pending_typed_chat_search
            .as_ref()
            .is_some_and(|(_, q)| q == trimmed)
            && self.session.search.chat_search.query == trimmed
        {
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        if self.pending_typed_chat_search.is_none()
            && self.session.search.chat_search.query == trimmed
            && matches!(
                self.session.search.chat_search.status,
                SearchStatus::Searching
                    | SearchStatus::Ready
                    | SearchStatus::Empty
                    | SearchStatus::Failed
            )
        {
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        let _search_gen = self.session.search.chat_search.begin_query(trimmed);
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
        let _ = self.session.search.chat_search.begin_query(query);
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
        if !self.chats_path_active() || !self.session.search.chat_search.open {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.search.chat_search.sender = sender;
        self.session.close_from_picker();
        let query = self.session.search.chat_search.query.clone();
        if query.is_empty() && !self.session.search.chat_search.has_criteria() {
            self.clear_chat_search_debounce();
            self.session.search.chat_search.clear_query();
            return Ok(ChatSearchQueryOutcome::Unchanged);
        }
        self.restart_chat_search(&query)
    }

    /// Pick the media tab of the in-chat search; the search reruns.
    pub fn set_chat_search_media(
        &mut self,
        media: SearchMediaKind,
    ) -> Result<ChatSearchQueryOutcome, ConnectSendError> {
        if !self.chats_path_active() || !self.session.search.chat_search.open {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.search.chat_search.media = media;
        let query = self.session.search.chat_search.query.clone();
        if query.is_empty() && !self.session.search.chat_search.has_criteria() {
            self.clear_chat_search_debounce();
            self.session.search.chat_search.clear_query();
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
        let Some(chat_id) = self
            .session
            .search
            .chat_search
            .chat_id
            .or(self.session.open_chat)
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let Some(picker) = self.session.search.chat_search.from_picker.as_ref() else {
            return Ok(None);
        };
        if picker.query == query && (picker.request.is_some() || !picker.members.is_empty()) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::SearchFromMembers, Some(chat_id));
        if let Some(picker) = self.session.search.chat_search.from_picker.as_mut() {
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
                if let Some(picker) = self.session.search.chat_search.from_picker.as_mut() {
                    picker.request = None;
                }
                Err(err)
            }
        }
    }

    /// Fetch the next older page of hits (the first page holds
    /// [`CHAT_SEARCH_LIMIT`]; "N of M" counts the server's total).
    pub fn load_more_chat_search(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.search.chat_search.can_load_more() {
            return Ok(None);
        }
        let Some(chat_id) = self
            .session
            .search
            .chat_search
            .chat_id
            .or(self.session.open_chat)
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let state = &self.session.search.chat_search;
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
        self.session.search.chat_search.loading_more = true;
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
                self.session.search.chat_search.loading_more = false;
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
        if let Some(calendar) = self.session.search.history_calendar.as_mut() {
            calendar.show_month(month);
        }
        self.fetch_calendar_page()
    }

    /// Page `getChatMessageCalendar` back until the displayed month is covered.
    pub fn fetch_calendar_page(&mut self) -> Result<(), ConnectSendError> {
        let Some(calendar) = self.session.search.history_calendar.as_ref() else {
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
        if let Some(calendar) = self.session.search.history_calendar.as_mut() {
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
                if let Some(calendar) = self.session.search.history_calendar.as_mut() {
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
        let Some(calendar) = self.session.search.history_calendar.take() else {
            return Ok(None);
        };
        self.session.search.date_jump_note = None;
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
        let Some(chat_id) = self
            .session
            .search
            .chat_search
            .chat_id
            .or(self.session.open_chat)
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let search_gen = self.session.search.chat_search.generation;
        let extra = self.session.request_chat_search(
            RequestPurpose::SearchChatMessages,
            chat_id,
            search_gen,
        );
        let (sender, media) = (
            self.session.search.chat_search.sender,
            self.session.search.chat_search.media,
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
                    .search
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
                if let Some(chat_id) = self
                    .session
                    .search
                    .chat_search
                    .chat_id
                    .or(self.session.open_chat)
                {
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
            .search
            .chat_search
            .selected_hit()
            .map(|hit| hit.message_id)
        else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    pub fn chat_search_newer(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(message_id) = self.session.search.chat_search.select_newer() else {
            return Ok(None);
        };
        self.jump_to_chat_search_message(message_id)
    }

    pub fn chat_search_older(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        // Prefetch the next page before the last loaded hit is reached.
        let state = &self.session.search.chat_search;
        if state
            .selected
            .is_some_and(|i| i + CHAT_SEARCH_PREFETCH >= state.hits.len())
        {
            let _ = self.load_more_chat_search();
        }
        let Some(message_id) = self.session.search.chat_search.select_older() else {
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
                self.session.search.chat_search.jump =
                    crate::state::ChatSearchJump::Missing { message_id };
                Err(err)
            }
        }
    }
}
