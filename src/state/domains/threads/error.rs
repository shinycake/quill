//! Failed requests for forum topics, comment threads and Saved Messages.
use crate::lang::{keys, tr};
use crate::premium_limits::PremiumLimitKind;
use crate::state::*;

impl Session {
    /// Reacts to a failed threads request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_threads_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::StartBotThread) => {
                if let Some(request) = extra {
                    let line =
                        call_request_error_line(err, tr(keys::Q_CHAT_COULDN_T_START_A_NEW_THREAD));
                    self.bot_thread_answered(
                        request,
                        pending.map(|p| p.asked_in).unwrap_or_default(),
                        Err(line),
                    );
                }
            }
            // tdesktop `SublistsPinsLimitBox`: Saved Messages has its own
            // pin limit.
            Some(RequestPurpose::Threads(ThreadsPurpose::ToggleSavedMessagesTopicPinned {
                ..
            })) if err.premium_limit == Some(PremiumLimitKind::Pins) => {
                self.open_premium_limit(PremiumLimitKind::SavedPins, 0);
            }
            Some(RequestPurpose::GetTopicHistory | RequestPurpose::GetTopicHistoryNewer) => {
                if let Some(pending) = pending {
                    self.fail_topic_page(pending);
                }
            }
            // Saved Messages: a 404 from `loadSavedMessagesTopics` says all
            // sublists were loaded; it is not a failure.
            Some(RequestPurpose::LoadSavedMessagesTopics) => {
                let asked = pending.is_some_and(|pending| {
                    self.threads.saved.page_failed(pending.id, err.code == 404)
                });
                if asked && err.code != 404 {
                    // Not sent as far as the pump goes: it asks again when
                    // the connection is back or Saved Messages opens again.
                    self.chats_state
                        .chat_action_error
                        .push(call_request_error_line(err, "Could not load saved chats"));
                }
            }
            // The picker ends in "no icons" and the next open asks again.
            Some(RequestPurpose::GetForumTopicDefaultIcons) => {
                self.threads.forum_topic_icons.failed(
                    pending.map(|p| p.id),
                    format!("error {}", err.code),
                    false,
                );
            }
            Some(RequestPurpose::GetSavedMessagesTags { topic_id: 0 }) => {
                self.threads.saved.tags.failed(
                    pending.map(|p| p.id),
                    format!("error {}", err.code),
                    false,
                );
                self.chats_state
                    .chat_action_error
                    .push(call_request_error_line(
                        err,
                        "Could not complete that action",
                    ));
            }
            Some(RequestPurpose::LoadDirectMessagesChatTopics) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && self
                        .threads
                        .direct
                        .page_failed(chat_id.0, pending.map(|p| p.id), err.code)
                {
                    if err.code == 404 {
                        self.view_generation.bump();
                    } else {
                        self.chats_state
                            .chat_action_error
                            .push(call_request_error_line(err, "Could not load chats"));
                    }
                }
            }
            Some(RequestPurpose::Threads(
                ThreadsPurpose::DeleteDirectMessagesChatTopicHistory { .. }
                | ThreadsPurpose::SetDirectMessagesChatTopicMarkedAsUnread { .. }
                | ThreadsPurpose::ReadDirectMessagesChatTopic { .. }
                | ThreadsPurpose::ToggleDirectMessagesChatTopicCanSendUnpaidMessages { .. },
            )) => {
                self.chats_state
                    .chat_action_error
                    .push(call_request_error_line(
                        err,
                        "Could not complete that action",
                    ));
            }
            // The rows show "No chats" and do not ask again.
            Some(RequestPurpose::GetForumTopics) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.threads.forum_topics.failed_request(
                        chat_id.0,
                        pending.map(|p| p.id),
                        error_reason(err),
                    );
                }
            }
            Some(RequestPurpose::GetForumTopicLink) => {
                self.messages
                    .message_link_error
                    .push(call_request_error_line(err, "Could not get the topic link"));
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
                if let Some(request) = extra {
                    self.topic_action_answered(
                        request,
                        pending.map(|p| p.asked_in).unwrap_or_default(),
                        false,
                    );
                }
                self.chats_state
                    .chat_action_error
                    .push(call_request_error_line(
                        err,
                        "Could not complete that action",
                    ));
            }
            _ => {}
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::SearchSavedMessages { .. })
        ) {
            self.fail_saved_tag_page(pending);
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::GetSavedMessagesTopicHistory { .. })
        ) {
            self.fail_saved_topic_page(pending);
        }
        // A failed thread request marks the open thread view.
        self.fail_thread(
            pending,
            call_request_error_line(err, "Could not load comments"),
        );
    }
}
