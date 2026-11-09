//! Join requests: one request handled, refused, or the pages of the
//! chat-wide list dropped after a bulk action.
use crate::state::*;

/// Server errors that say the request is not there to process any more:
/// the user withdrew it, or another admin or device handled it first.
const GONE: [&str; 2] = ["HIDE_REQUESTER_MISSING", "USER_ALREADY_PARTICIPANT"];

impl Session {
    /// Drop the chat-wide join-request pages in flight: their answers
    /// were asked before a bulk Approve all or Dismiss all and would bring
    /// handled requests back.
    pub(crate) fn drop_chat_join_request_pages(&mut self, chat_id: ChatId) {
        for purpose in [
            RequestPurpose::GetChatJoinRequests,
            RequestPurpose::GetMoreChatJoinRequests,
        ] {
            self.requests.take_purpose_for_chat(purpose, chat_id);
        }
        self.groups.join_request_latest.remove(&chat_id.0);
    }

    /// `user_id`'s request in `chat_id` is handled (or gone): it leaves
    /// the chat-wide list and the open link's list.
    pub(crate) fn forget_join_request(&mut self, chat_id: ChatId, user_id: i64) {
        if let Some(JoinRequestFetch::Loaded(list)) = self.groups.join_requests.get_mut(&chat_id.0)
        {
            let before = list.requests.len();
            list.requests.retain(|r| r.user_id != user_id);
            if list.requests.len() < before {
                list.total_count = list.total_count.saturating_sub(1);
            }
        }
        if let Some(state) = self.groups.invite_pages.join_requests_mut(chat_id.0) {
            let before = state.requests.len();
            state.requests.retain(|r| r.user_id != user_id);
            if state.requests.len() < before {
                state.total_count = state.total_count.saturating_sub(1);
            }
        }
    }

    /// A refused approve or dismiss of one request: the loaded lists stay,
    /// the row goes when the request is gone, and the line shows where the
    /// other invite-link errors do.
    pub(crate) fn fail_join_request(&mut self, err: &TdError, chat_id: ChatId, user_id: i64) {
        if err
            .server_type
            .as_deref()
            .is_some_and(|kind| GONE.contains(&kind))
        {
            self.forget_join_request(chat_id, user_id);
        }
        self.groups.invite_link_error = Some(call_request_error_line(
            err,
            "Could not process join request",
        ));
    }
}
