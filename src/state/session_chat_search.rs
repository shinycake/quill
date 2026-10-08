//! In-chat search.
use super::*;

impl Session {
    /// Resolve a hit: already loaded, tombstoned/deleted, or needs `getChatHistory` around.
    pub fn begin_chat_search_jump(&mut self, message_id: MessageId) -> ChatSearchJumpNeed {
        self.chat_search.jump_serial += 1;
        let Some(chat_id) = self.chat_search.chat_id.or(self.open_chat) else {
            self.chat_search.jump = ChatSearchJump::Missing { message_id };
            return ChatSearchJumpNeed::Missing;
        };
        self.chat_search.select_message(message_id);
        let history = self.histories.entry(chat_id.0).or_default();
        if history.is_tombstone(message_id) {
            self.chat_search.jump = ChatSearchJump::Missing { message_id };
            return ChatSearchJumpNeed::Missing;
        }
        if history.contains(message_id) {
            self.chat_search.jump = ChatSearchJump::Ready { message_id };
            return ChatSearchJumpNeed::AlreadyReady;
        }
        self.chat_search.jump = ChatSearchJump::Loading { message_id };
        ChatSearchJumpNeed::LoadAround
    }

    pub fn apply_local_chat_search_filter(&mut self, query: &str) {
        let Some(chat_id) = self.open_chat else {
            return;
        };
        if !self.chat_search.open {
            self.chat_search.open_for(chat_id);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.chat_search.clear_query();
            return;
        }
        let _ = self.chat_search.begin_query(trimmed);
        let needle = trimmed.to_lowercase();
        let hits: Vec<SearchMessageHit> = self
            .histories
            .get(&chat_id.0)
            .map(|history| {
                history
                    .ordered()
                    .into_iter()
                    .rev()
                    .filter(|message| message.content.preview().to_lowercase().contains(&needle))
                    .map(|message| SearchMessageHit {
                        sender: message.sender,
                        chat_id: message.chat_id,
                        message_id: message.id,
                        preview: effective_preview(message),
                        is_outgoing: message.is_outgoing,
                        content: message.content.clone(),
                        author_signature: message.author_signature.clone(),
                        reply_to: message.reply_to.clone(),
                        forward_info: message.forward_info.clone(),
                        extras: message.extras.clone(),
                        interaction_info: message.interaction_info.clone(),
                        is_pinned: message.is_pinned,
                        media_album_id: message.media_album_id,
                        reply_markup: message.reply_markup.clone(),
                        self_destruct: message.self_destruct,
                        auto_delete: message.auto_delete,
                        date: message.date,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total = hits.len() as i32;
        self.chat_search
            .accept_hits(hits, total, MessageId(0), false);
        if let Some(id) = self.chat_search.selected_hit().map(|hit| hit.message_id) {
            let _ = self.begin_chat_search_jump(id);
        }
    }

    /// Newest pinned message in the open chat's loaded history.
    pub fn open_chat_pinned_message(&self) -> Option<&HistoryMessage> {
        let chat_id = self.open_chat?;
        self.histories.get(&chat_id.0)?.newest_pinned()
    }

    /// Insert a found message into that chat's history so open-chat can show it
    /// without a separate history pagination scheme.
    pub fn promote_search_message(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(index) = self
            .search
            .messages
            .iter()
            .position(|hit| hit.chat_id == chat_id && hit.message_id == message_id)
        else {
            return;
        };
        let hit = self.search.messages[index].clone();
        let row = hit.into_history();
        self.index_poll(&row);
        self.histories.entry(chat_id.0).or_default().upsert(row);
    }

    /// Main-list chats whose title contains `query` (case-insensitive). Demo-only
    /// local filter when no live TDLib replies are injected.
    pub fn local_search_chats(&self, query: &str) -> Vec<&ChatSummary> {
        let needle = query.trim().to_lowercase();
        self.ordered_chats()
            .into_iter()
            .filter(|chat| needle.is_empty() || chat.title.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn apply_local_search_filter(&mut self, query: &str) {
        self.search.open = true;
        self.search.query = query.to_string();
        self.search.generation = self.search.generation.saturating_add(1);
        self.search.clear_results();
        self.search.recents = query.trim().is_empty();
        self.search.chat_ids = self
            .local_search_chats(query)
            .into_iter()
            .map(|chat| chat.id)
            .collect();
        self.search.chats_done = true;
        self.search.messages_done = true;
        if query.trim().is_empty() {
            self.search.chat_ids.clear();
            self.search.status = SearchStatus::Idle;
        } else {
            self.search.finish_if_complete();
        }
    }

    pub fn begin_close(&mut self) {
        self.shutdown = ShutdownPhase::CloseRequested;
    }

    pub fn begin_logout(&mut self) {
        self.account_export = None;
        self.requests.invalidate_account();
        self.account_generation.bump();
        self.shutdown = ShutdownPhase::CloseRequested;
    }
}

pub(crate) fn history_message(message: ParsedMessage, pending: bool) -> HistoryMessage {
    HistoryMessage {
        sender: message.sender,
        id: message.id,
        chat_id: message.chat_id,
        is_outgoing: message.is_outgoing,
        date: message.date,
        content: message.content,
        pending,
        reply_to: message.reply_to,
        forward_info: message.forward_info,
        extras: message.extras,
        interaction_info: message.interaction_info,
        is_pinned: message.is_pinned,
        media_album_id: message.media_album_id,
        reply_markup: message.reply_markup,
        self_destruct: message.self_destruct,
        auto_delete: message.auto_delete,
        author_signature: message.author_signature,
        failed: false,
        can_retry: message.can_retry,
        ephemeral: message.ephemeral,
    }
}
