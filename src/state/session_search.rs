//! Global search and history fetch.
use super::*;

impl Session {
    pub fn request_search(&mut self, purpose: RequestPurpose, search_generation: u64) -> RequestId {
        self.requests
            .register_search(self.account_generation, purpose, search_generation)
    }

    pub fn request_chat_search(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        search_generation: u64,
    ) -> RequestId {
        self.requests.register_chat_search(
            self.account_generation,
            purpose,
            chat_id,
            search_generation,
        )
    }

    pub fn request_history_around(&mut self, chat_id: ChatId, message_id: MessageId) -> RequestId {
        self.requests.register_around(
            self.account_generation,
            chat_id,
            self.view_generation,
            message_id,
        )
    }

    pub fn open_search(&mut self) {
        self.search.search.open_field();
    }

    pub fn close_search(&mut self) {
        self.search.search.close();
    }

    pub fn open_chat_search(&mut self) -> bool {
        let Some(chat_id) = self.open_chat else {
            return false;
        };
        if !self
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported())
        {
            return false;
        }
        self.search.chat_search.open_for(chat_id);
        true
    }

    pub fn close_chat_search(&mut self) {
        self.search.chat_search.close();
    }

    pub(crate) fn apply_history_around(
        &mut self,
        pending: &PendingRequest,
        messages: &[ParsedMessage],
        seq: u64,
    ) {
        if self.history_page_detached(pending, seq) {
            // The window it was for was replaced (a jump down, an own send,
            // a topic switch) or left: the jump it served ends with it,
            // rather than wait on "Loading message..." for an answer that
            // was just dropped.
            if let Some(message_id) = pending.around_message_id {
                self.abandon_history_around(pending.chat_id, message_id);
            }
            return;
        }
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        // Nothing older in an around page means only that TDLib's run in
        // memory ends there: older pages still ask the server.
        self.store_first_page(chat_id, messages);
        if let Some(message_id) = pending.around_message_id {
            self.finish_history_around(Some(pending), message_id, false, seq);
        }
    }

    /// The around page for `message_id` in `chat_id` was dropped unused:
    /// the search jump and the date jump waiting on it end.
    fn abandon_history_around(&mut self, chat_id: Option<ChatId>, message_id: MessageId) {
        let search = &mut self.search.chat_search;
        if matches!(
            search.jump,
            ChatSearchJump::Loading { message_id: current } if current == message_id
        ) && (search.chat_id.is_none() || search.chat_id == chat_id)
        {
            search.jump = ChatSearchJump::None;
        }
        if let Some((pending_chat, pending_id, _)) = self.search.date_jump_pending
            && pending_id == message_id
            && Some(pending_chat) == chat_id
        {
            self.search.date_jump_pending = None;
        }
    }

    pub(crate) fn finish_history_around(
        &mut self,
        pending: Option<&PendingRequest>,
        message_id: MessageId,
        _error: bool,
        seq: u64,
    ) {
        if !matches!(
            self.search.chat_search.jump,
            ChatSearchJump::Loading { message_id: current } if current == message_id
        ) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: pending.map(|p| p.id.0),
                seq: Some(seq),
                note: "stale-chat-search-jump",
            });
            return;
        }
        let Some(chat_id) = pending
            .and_then(|p| p.chat_id)
            .or(self.search.chat_search.chat_id)
        else {
            return;
        };
        let history = self.history_entry(chat_id);
        // Tombstone, 404/error, or around-load without the id: deleted/inaccessible.
        self.search.chat_search.jump = if history.contains(message_id) {
            ChatSearchJump::Ready { message_id }
        } else {
            ChatSearchJump::Missing { message_id }
        };
        self.resolve_date_jump(message_id);
    }
}
