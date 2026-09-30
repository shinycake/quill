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
        self.search.open_field();
    }

    pub fn close_search(&mut self) {
        self.search.close();
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
        self.chat_search.open_for(chat_id);
        true
    }

    pub fn close_chat_search(&mut self) {
        self.chat_search.close();
    }

    pub(crate) fn apply_history_around(
        &mut self,
        pending: &PendingRequest,
        messages: &[ParsedMessage],
        seq: u64,
    ) {
        if pending.view_generation != Some(self.view_generation) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: Some(pending.id.0),
                seq: Some(seq),
                note: "stale-view-generation",
            });
            return;
        }
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        if self.open_chat != Some(chat_id) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: Some(pending.id.0),
                seq: Some(seq),
                note: "stale-chat-history",
            });
            return;
        }
        for message in messages {
            self.upsert_message(message.clone(), false);
        }
        if let Some(message_id) = pending.around_message_id {
            self.finish_history_around(Some(pending), message_id, false, seq);
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
            self.chat_search.jump,
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
        let Some(chat_id) = pending.and_then(|p| p.chat_id).or(self.chat_search.chat_id) else {
            return;
        };
        let history = self.histories.entry(chat_id.0).or_default();
        // Tombstone, 404/error, or around-load without the id: deleted/inaccessible.
        self.chat_search.jump = if history.contains(message_id) {
            ChatSearchJump::Ready { message_id }
        } else {
            ChatSearchJump::Missing { message_id }
        };
    }
}
