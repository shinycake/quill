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
                self.chats_state.chat_action_error.push(format!(
                    "could not remove the recent search (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::RemoveTopChat | RequestPurpose::SetTopChatsDisabled) => {
                self.chats_state.chat_action_error.push(format!(
                    "could not update frequent contacts (error {})",
                    err.code
                ));
            }
            _ => {}
        }
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetRecommendedChats) => {
                self.search
                    .search
                    .tabs
                    .recommended
                    .fail(format!("error {}", err.code));
            }
            Some(RequestPurpose::SearchMessagesMore | RequestPurpose::SearchPublicPostsMore)
                if self.search.search.awaits(pending) =>
            {
                self.search.search.fail_page();
            }
            _ => {}
        }
        if self.search.search.awaits(pending) {
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
                Some(RequestPurpose::SearchChatsOnServer) => {
                    self.search.search.server_chats.cancel();
                }
                // No quota line: the search goes on without it.
                Some(RequestPurpose::GetPublicPostSearchLimits) => {
                    self.search.search.tabs.limits.cancel();
                }
                Some(RequestPurpose::SearchPublicChats) => {
                    self.search.search.accept_public_chats(Vec::new(), true);
                }
                _ => {}
            }
        }
        if self.search.chat_search.more_waits_for(pending)
            && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessagesMore)
        {
            // The page stays askable: the Retry strip asks it again.
            self.search.chat_search.fail_more();
        }
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetChatMessageByDate) => {
                self.fail_date_jump(err.code == 404, pending)
            }
            Some(RequestPurpose::Search(SearchPurpose::GetChatMessageCalendar)) => {
                self.fail_message_calendar(pending)
            }
            Some(RequestPurpose::SearchFromMembers) => {
                // Only the search the picker waits for ends; an older one's
                // error keeps the newer answer coming.
                if let Some(picker) = self
                    .search
                    .chat_search
                    .open_mut()
                    .and_then(|open| open.from_picker.as_mut())
                {
                    picker.members.failed(
                        pending.map(|p| p.id),
                        crate::state::request_errors::error_reason(err),
                        false,
                    );
                }
            }
            _ => {}
        }
        if self.search.chat_search.search_waits_for(pending)
            && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessages)
        {
            self.search
                .chat_search
                .accept_hits(Vec::new(), 0, MessageId(0), true);
        }
    }
}
