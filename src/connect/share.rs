//! Connect driver: share box search and the "send as" identity.
use super::*;
use crate::ids::ChatId;
use crate::state::RequestPurpose;
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests::{
    get_chat_available_message_senders, search_chats, search_chats_on_server,
    set_chat_message_sender,
};

/// Rows asked of each search (the box shows a short scrolling list).
const SHARE_SEARCH_LIMIT: i32 = 30;

impl<S: JsonSender> ConnectDriver<S> {
    /// Share box search: `searchChats` (known chats, contacts) plus
    /// `searchChatsOnServer`. An empty query clears the extra rows. The
    /// reducer drops answers of an older query.
    pub fn search_share_chats(&mut self, query: &str) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let query = query.trim();
        if query.is_empty() {
            self.session.share_search.clear();
            return Ok(());
        }
        if self.session.share_search.query == query {
            return Ok(());
        }
        let local = self.session.request(RequestPurpose::SearchShareChats, None);
        let server = self
            .session
            .request(RequestPurpose::SearchShareChatsOnServer, None);
        self.session.share_search.begin(query, local, server);
        let result = self
            .sender
            .send_json(&search_chats(local, query, SHARE_SEARCH_LIMIT))
            .and_then(|()| {
                self.sender
                    .send_json(&search_chats_on_server(server, query, SHARE_SEARCH_LIMIT))
            });
        if result.is_err() {
            self.session.requests.take(local);
            self.session.requests.take(server);
            self.session.share_search.clear();
        }
        result
    }

    /// `getChatAvailableMessageSenders` for the send-as picker; skipped
    /// while an earlier request for the chat is in flight.
    pub fn get_chat_available_message_senders(
        &mut self,
        chat_id: ChatId,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || !self.session.can_choose_message_sender(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatAvailableMessageSenders, chat_id)
        {
            return Ok(());
        }
        let extra = self.session.request(
            RequestPurpose::GetChatAvailableMessageSenders,
            Some(chat_id),
        );
        self.sender
            .send_json(&get_chat_available_message_senders(extra, chat_id))
            .inspect_err(|_| {
                self.session.requests.take(extra);
            })
    }

    /// `setChatMessageSender`; the choice becomes current when
    /// `updateChatMessageSender` arrives. Only senders from the loaded
    /// list (and not premium-locked) are accepted.
    pub fn set_chat_message_sender(
        &mut self,
        chat_id: ChatId,
        sender: MessageSender,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let allowed = self
            .session
            .available_message_senders(chat_id)
            .iter()
            .any(|entry| entry.sender == sender && !entry.needs_premium);
        if !allowed {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatMessageSender, Some(chat_id));
        self.sender
            .send_json(&set_chat_message_sender(extra, chat_id, sender))
            .inspect_err(|_| {
                self.session.requests.take(extra);
            })
    }
}
