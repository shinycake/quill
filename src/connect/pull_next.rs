//! Connect driver: the pull to the next unread channel (`pull_next`).
use super::*;
use crate::ids::{ChatId, MessageId};
use crate::state::fetch::Fetch;
use crate::state::{HistoryPreload, RequestPurpose};
use crate::telegram::requests::get_chat_history;

impl<S: JsonSender> ConnectDriver<S> {
    /// tdesktop `Support::SendPreloadRequest` and `PreloadPinnedBar`: while
    /// the user pulls toward `chat_id`, load a page of its history around
    /// the read position (or its newest page when everything is read) and
    /// its pinned messages, so the chat opens ready. Nothing is marked
    /// read: the chat is not opened.
    pub fn preload_next_channel(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let around = self
            .session
            .chats
            .get(&chat_id.0)
            .filter(|chat| chat.unread_count > 0)
            .map_or(MessageId(0), |chat| chat.last_read_inbox_message_id);
        let purpose = RequestPurpose::PreloadHistory;
        let asked = matches!(
            self.session.chat_list.history_preloads.get(&chat_id.0),
            Some(HistoryPreload::Loading(_))
        );
        if !asked {
            let offset = if around.0 != 0 {
                -crate::pull_next::PRELOAD_MESSAGES / 2
            } else {
                0
            };
            let extra = self.session.request(purpose, Some(chat_id));
            self.session
                .chat_list
                .history_preloads
                .insert(chat_id.0, HistoryPreload::Loading(extra));
            if let Err(err) = self.sender.send_json(&get_chat_history(
                extra,
                chat_id,
                around,
                offset,
                crate::pull_next::PRELOAD_MESSAGES,
                false,
            )) {
                self.session.requests.take(extra);
                self.session.chat_list.history_preloads.remove(&chat_id.0);
                return Err(err);
            }
        }
        // Asked once per pull target: a failed fetch is not sent again by
        // every preload tick, the chat opening asks for it.
        if self
            .session
            .messages
            .pinned_messages
            .get(&chat_id.0)
            .is_none_or(Fetch::is_idle)
        {
            self.fetch_pinned_messages(chat_id)?;
        }
        Ok(())
    }

    /// The next channel's history page arrived (tdesktop
    /// `History::isReadyFor(ShowAtUnreadMsgId)`).
    pub fn next_channel_history_ready(&self, chat_id: ChatId) -> bool {
        self.session.chat_list.history_preloaded(chat_id.0)
    }

    /// The next channel's pinned messages arrived (tdesktop
    /// `PinnedBarReady`).
    pub fn next_channel_pinned_ready(&self, chat_id: ChatId) -> bool {
        self.session
            .messages
            .pinned_messages
            .get(&chat_id.0)
            .is_some_and(|pinned| matches!(pinned, Fetch::Ready(_)))
    }

    /// tdesktop `forum->requestTopics()` when the pull in a topic finds no
    /// unread topic before the list loaded.
    pub fn request_forum_topics(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        self.maybe_fetch_forum_topics(chat_id)
    }
}
