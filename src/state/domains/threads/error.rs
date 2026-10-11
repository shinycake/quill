//! Failed requests for forum topics, comment threads and Saved Messages.
use crate::state::*;

impl Session {
    /// Reacts to a failed threads request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_threads_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            // Saved Messages: a 404 from `loadSavedMessagesTopics` says all
            // sublists were loaded; it is not a failure.
            Some(RequestPurpose::LoadSavedMessagesTopics) => {
                if err.code == 404 {
                    self.threads.saved.topics_exhausted = true;
                } else {
                    self.chats_state.chat_action_error =
                        Some(call_request_error_line(err, "Could not load saved chats"));
                }
            }
            Some(RequestPurpose::GetForumTopicLink) => {
                self.messages.message_link_error =
                    Some(call_request_error_line(err, "Could not get the topic link"));
            }
            Some(
                RequestPurpose::ToggleChatViewAsTopics
                | RequestPurpose::SetPinnedForumTopics
                | RequestPurpose::Threads(ThreadsPurpose::ReadAllForumTopicMentions { .. })
                | RequestPurpose::Threads(ThreadsPurpose::ReadAllForumTopicReactions { .. })
                | RequestPurpose::Threads(ThreadsPurpose::UnpinAllForumTopicMessages { .. })
                | RequestPurpose::Threads(ThreadsPurpose::DeleteSavedMessagesTopicHistory { .. })
                | RequestPurpose::Threads(ThreadsPurpose::ToggleSavedMessagesTopicPinned { .. })
                | RequestPurpose::SetSavedMessagesTagLabel
                | RequestPurpose::GetSavedMessagesTags { .. }
                | RequestPurpose::GetSavedMessagesTopicHistory { .. }
                | RequestPurpose::SearchSavedMessages { .. },
            ) => {
                self.chats_state.chat_action_error = Some(call_request_error_line(
                    err,
                    "Could not complete that action",
                ));
            }
            _ => {}
        }
        // A failed thread request marks the open thread view.
        self.fail_thread(
            pending,
            call_request_error_line(err, "Could not load comments"),
        );
    }
}
