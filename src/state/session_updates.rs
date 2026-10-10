//! Update application: auth, positions, message upserts.
use super::*;

impl Session {
    pub(crate) fn set_auth(&mut self, state: AuthorizationState) {
        if self.auth != state {
            self.requests.invalidate_auth();
            // Download requests were dropped and files may be cleared
            // below: every avatar gets one fresh look.
            self.avatar_rescan = true;
        }
        if matches!(
            state,
            AuthorizationState::LoggingOut
                | AuthorizationState::Closing
                | AuthorizationState::Closed
        ) {
            self.settings.account_export = None;
        }
        // List paging belongs to one authorization. TDLib only leaves Ready
        // through LoggingOut / Closing / Closed, so a later Ready (another
        // login in this Session) must page every list from the start; the
        // Ready entry resets too in case the leaving state was missed.
        let entering_ready =
            matches!(state, AuthorizationState::Ready) && self.auth != AuthorizationState::Ready;
        if entering_ready
            || matches!(
                state,
                AuthorizationState::LoggingOut | AuthorizationState::Closed
            )
        {
            self.chats_exhausted = false;
            self.archive_chats_exhausted = false;
            self.folder_chats_exhausted.clear();
        }
        if matches!(state, AuthorizationState::Closed) {
            self.shutdown = ShutdownPhase::Closed;
            self.requests.invalidate_account();
            self.account_generation.bump();
            self.files.clear();
            self.downloading.clear();
            self.stalled_auto_downloads.clear();
            self.download_extras.clear();
            self.search.close();
            self.chat_search.close();
            self.in_flight_forward = None;
            self.queued_forward_flights.clear();
            self.chat_message_sender.clear();
            self.send_as_options.clear();
            self.share_search = ShareSearch::default();
            self.last_forward = None;
        }
        if matches!(state, AuthorizationState::LoggingOut) {
            self.requests.invalidate_account();
            self.search.close();
            self.chat_search.close();
            self.in_flight_forward = None;
        }
        if matches!(state, AuthorizationState::Closing) {
            self.shutdown = ShutdownPhase::WaitingClosed;
        }
        self.auth = state;
        self.auth_view = view_for(&self.auth);
        self.auth_state.last_auth_error = None;
    }

    pub(crate) fn apply_position_fields(&mut self, pos: ChatPositionUpdate) {
        // A position on one list is not an eviction from the other.
        // A single `updateChatPosition` updates only that list. A full
        // `updateChatLastMessage` positions set replaces both memberships.
        let chat = self
            .chats
            .entry(pos.chat_id.0)
            .or_insert_with(|| placeholder_chat(pos.chat_id));
        match pos.list {
            ChatList::Main => {
                if pos.order == 0 {
                    chat.in_main_list = false;
                } else {
                    chat.order = pos.order;
                    chat.is_pinned = pos.is_pinned;
                    chat.in_main_list = true;
                }
            }
            ChatList::Archive => {
                if pos.order == 0 {
                    chat.in_archive = false;
                } else {
                    chat.archive_order = pos.order;
                    chat.archive_is_pinned = pos.is_pinned;
                    chat.in_archive = true;
                }
            }
            // Phase 7.1: folder membership is positional, like Main/Archive.
            // `getChatListsToAddChat` is *not* folder membership — it lists
            // chat lists a chat can be added to for `addChatToList`.
            ChatList::Folder(folder_id) => {
                if pos.order == 0 {
                    chat.folder_positions.remove(&folder_id);
                } else {
                    chat.folder_positions.insert(folder_id, pos.order);
                }
            }
            ChatList::Unknown => {}
        }
    }

    pub(crate) fn replace_main_list_from_positions(
        &mut self,
        chat_id: ChatId,
        positions: &[ChatPositionUpdate],
    ) {
        match positions
            .iter()
            .find(|pos| pos.list == ChatList::Main)
            .cloned()
        {
            Some(pos) => self.apply_position_fields(pos),
            None => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.in_main_list = false;
                }
            }
        }
        match positions
            .iter()
            .find(|pos| pos.list == ChatList::Archive)
            .cloned()
        {
            Some(pos) => self.apply_position_fields(pos),
            None => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.in_archive = false;
                }
            }
        }
        // Folder positions are a full set too: drop folder ids that are no
        // longer present, then apply the ones that are.
        let folder_ids: HashSet<i32> = positions
            .iter()
            .filter_map(|pos| match pos.list {
                ChatList::Folder(id) => Some(id),
                _ => None,
            })
            .collect();
        if let Some(chat) = self.chats.get_mut(&chat_id.0) {
            chat.folder_positions
                .retain(|id, _| folder_ids.contains(id));
        }
        for pos in positions {
            if matches!(pos.list, ChatList::Folder(_)) {
                self.apply_position_fields(pos.clone());
            }
        }
    }

    pub(crate) fn upsert_message(&mut self, message: ParsedMessage, pending: bool) {
        self.remember_files(&message.files);
        let chat_id = message.chat_id;
        // Slice bots-games: remember games seen in a bot's chat so the bot
        // info panel can offer to send them. Keyed by the chat's bot user
        // id — only short names TDLib actually delivered are cached, so
        // `inputMessageGame` never gets an invented short name.
        if let MessageContent::Game(game) = &message.content
            && !game.short_name.is_empty()
            && let Some(bot_id) = self.bot_user_id_for_chat(chat_id)
        {
            let games = self.bot_games.entry(bot_id).or_default();
            if !games.iter().any(|g| g.short_name == game.short_name) {
                games.push(GameInfo {
                    short_name: game.short_name.clone(),
                    title: game.title.clone(),
                });
            }
        }
        let topic_id = message.topic_id;
        let row = history_message(message, pending);
        self.index_poll(&row);
        let history = self.histories.entry(chat_id.0).or_default();
        history.upsert(row.clone());
        // Parity slice 4: a message addressed to a forum topic also lands
        // in that topic's history when the topic is loaded (the topic view
        // reads `topic_histories`, never the chat's main history). Missing
        // entries are left alone so the paging cursor stays fetch-owned.
        if let Some(topic_id) = topic_id
            && let Some(topic_history) = self.topic_histories.get_mut(&(chat_id.0, topic_id))
        {
            topic_history.upsert(row);
        }
    }

    /// Apply `edit` to every loaded copy of one message: the row in the
    /// chat's main history and the row in any loaded forum-topic history
    /// of that chat (the topic view reads `topic_histories` only, and
    /// per-message updates carry no topic id). Returns whether any copy
    /// was found.
    pub(crate) fn edit_loaded_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        mut edit: impl FnMut(&mut HistoryMessage),
    ) -> bool {
        let mut found = false;
        if let Some(message) = self
            .histories
            .get_mut(&chat_id.0)
            .and_then(|history| history.messages.get_mut(&message_id.0))
        {
            edit(message);
            found = true;
        }
        for ((topic_chat_id, _), topic) in self.topic_histories.iter_mut() {
            if *topic_chat_id == chat_id.0
                && let Some(message) = topic.messages.get_mut(&message_id.0)
            {
                edit(message);
                found = true;
            }
        }
        if let Some(thread) = self.thread.as_mut()
            && thread.chat_id == chat_id
            && let Some(message) = thread.history.messages.get_mut(&message_id.0)
        {
            edit(message);
            found = true;
        }
        if self.my_user_id == Some(chat_id.0) {
            if let Some(message) = self
                .saved
                .sublist
                .as_mut()
                .and_then(|view| view.history.messages.get_mut(&message_id.0))
            {
                edit(message);
                found = true;
            }
            if let Some(message) = self
                .saved
                .tag_search
                .as_mut()
                .and_then(|search| search.history.messages.get_mut(&message_id.0))
            {
                edit(message);
                found = true;
            }
        }
        found
    }

    /// Record a loaded poll row in `poll_messages` (see `apply_update_poll`).
    pub(crate) fn index_poll(&mut self, message: &HistoryMessage) {
        if let MessageContent::Poll(content) = &message.content {
            self.poll_messages
                .entry(content.poll.id)
                .or_default()
                .insert((message.chat_id.0, message.id.0));
        }
    }

    pub(crate) fn remember_files(&mut self, files: &[ParsedFile]) {
        for file in files {
            // Nested message files can still be idle while a download is in flight.
            self.upsert_file(file.clone(), false);
        }
    }
}
