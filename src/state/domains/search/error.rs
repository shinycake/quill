//! Failed requests for global and in-chat search, top chats and date jumps.
use crate::state::*;

impl Session {
    /// Reacts to a failed search request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_search_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::RemoveRecentlyFoundChat) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not remove the recent search (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::RemoveTopChat | RequestPurpose::SetTopChatsDisabled) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not update frequent contacts (error {})",
                    err.code
                ));
            }
            _ => {}
        }
        if self.search.search.matches_generation(pending) {
            match pending.map(|p| p.purpose) {
                Some(RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats) => {
                    self.search.search.accept_chats(Vec::new(), true);
                }
                Some(
                    RequestPurpose::SearchMessages
                    | RequestPurpose::SearchPublicPosts
                    | RequestPurpose::SearchPublicMessagesByTag,
                ) => {
                    self.search.search.accept_messages(Vec::new(), true);
                }
                // The supplement failing changes nothing the user sees.
                Some(RequestPurpose::SearchChatsOnServer) => {}
                Some(RequestPurpose::SearchPublicChats) => {
                    self.search.search.accept_public_chats(Vec::new(), true);
                }
                _ => {}
            }
        }
        if self.search.chat_search.matches_generation(pending)
            && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessagesMore)
        {
            self.search.chat_search.loading_more = false;
            self.search.chat_search.next_from_message_id = MessageId(0);
        }
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetChatMessageByDate) => self.fail_date_jump(err.code == 404),
            Some(RequestPurpose::Search(SearchPurpose::GetChatMessageCalendar { .. })) => {
                self.fail_message_calendar(pending)
            }
            Some(RequestPurpose::SearchFromMembers) => {
                if let Some(picker) = self.search.chat_search.from_picker.as_mut() {
                    picker.request = None;
                }
            }
            _ => {}
        }
        if self.search.chat_search.matches_generation(pending)
            && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessages)
        {
            self.search
                .chat_search
                .accept_hits(Vec::new(), 0, MessageId(0), true);
        }
    }
}
