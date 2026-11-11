#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ids::{ChatId, MessageId, RequestId, TopicId};
use crate::state::{DIRECT_TOPICS_PAGE, GroupsPurpose, RequestPurpose, ThreadsPurpose};
use crate::telegram::requests::{
    delete_direct_messages_chat_topic_history, get_chat_history, get_chat_member,
    get_direct_messages_chat_topic_history, get_direct_messages_chat_topic_revenue,
    load_direct_messages_chat_topics, route_into_direct_topic,
    set_direct_messages_chat_topic_is_marked_as_unread,
    toggle_direct_messages_chat_topic_can_send_unpaid_messages, view_messages,
};

impl<S: JsonSender> ConnectDriver<S> {
    pub(crate) fn main_history_request(
        &self,
        extra: RequestId,
        chat_id: ChatId,
        from: MessageId,
        offset: i32,
        limit: i32,
    ) -> String {
        match self.session.open_direct_topic(chat_id) {
            Some(topic) => {
                get_direct_messages_chat_topic_history(extra, chat_id, topic, from, offset, limit)
            }
            None => get_chat_history(extra, chat_id, from, offset, limit, false),
        }
    }

    pub(crate) fn direct_view_messages_source(&self, chat_id: ChatId) -> Option<&'static str> {
        self.session
            .open_direct_topic(chat_id)
            .map(|_| "messageSourceDirectMessagesChatTopicHistory")
    }

    /// In an admin's All view a send carries the conversation of the
    /// message it replies to: TDLib refuses a send without one ("Channel
    /// direct messages topic must be specified").
    pub(crate) fn direct_routed(&self, chat_id: ChatId, json: String) -> String {
        if !self.session.is_direct_messages_admin(chat_id)
            || self.session.open_direct_topic(chat_id).is_some()
        {
            return json;
        }
        route_into_direct_topic(&json, |message_id| {
            self.session
                .direct_topic_of_message(chat_id, MessageId(message_id))
                .map(|topic| TopicId::DirectMessages {
                    direct_messages_chat_topic_id: topic,
                })
        })
    }

    /// The first `loadDirectMessagesChatTopics` of an administered direct
    /// messages chat, so its row and tabs fill (tdesktop preloads every
    /// monoforum it lists).
    pub(crate) fn maybe_preload_direct_topics(&mut self, chat_id: ChatId) {
        if !self.session.is_direct_messages_admin(chat_id)
            || !self.session.threads.direct.never_asked(chat_id.0)
        {
            return;
        }
        let _ = self.load_more_direct_topics(chat_id);
    }

    /// Opening an administered direct messages chat whose first page failed
    /// asks for it again.
    pub(crate) fn retry_direct_topics_on_open(&mut self, chat_id: ChatId) {
        if self.session.is_direct_messages_admin(chat_id)
            && !self.session.threads.direct.preloaded(chat_id.0)
        {
            let _ = self.load_more_direct_topics(chat_id);
        }
    }

    /// The next page of conversations (the tabs scrolled to their end).
    pub fn load_more_direct_topics(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.is_direct_messages_admin(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::LoadDirectMessagesChatTopics;
        if self.session.threads.direct.exhausted(chat_id.0)
            || self.session.requests.has_purpose_for_chat(purpose, chat_id)
        {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let json = load_direct_messages_chat_topics(extra, chat_id, DIRECT_TOPICS_PAGE);
        let sent = self.send_json_request(extra, &json).map(Some);
        if sent.is_ok() {
            self.session.threads.direct.page_asked(chat_id.0);
        }
        sent
    }

    /// Show one conversation (or All) and load its first page.
    pub fn select_direct_topic(
        &mut self,
        chat_id: ChatId,
        topic: Option<i64>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.select_direct_topic(chat_id, topic) {
            return Ok(None);
        }
        if !self.session.chat_is_shown(chat_id) {
            return Ok(None);
        }
        self.fetch_history_for(chat_id)
    }

    /// "Mark as read" (tdesktop `SavedSublist::readTillEnd`): clear a hand
    /// set mark and view the last message with `force_read`. "Mark as
    /// unread" sets the mark.
    pub fn set_direct_topic_read(
        &mut self,
        chat_id: ChatId,
        topic_id: i64,
        read: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.is_direct_messages_admin(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(topic) = self.session.direct_topic(chat_id, topic_id) else {
            return Ok(None);
        };
        let (marked, unread, last) = (
            topic.is_marked_as_unread,
            topic.unread_count > 0,
            topic.last_message_id,
        );
        let mut sent = None;
        if marked == read {
            let purpose =
                RequestPurpose::Threads(ThreadsPurpose::SetDirectMessagesChatTopicMarkedAsUnread {
                    topic_id,
                });
            let extra = self.session.request(purpose, Some(chat_id));
            let json =
                set_direct_messages_chat_topic_is_marked_as_unread(extra, chat_id, topic_id, !read);
            sent = Some(self.send_json_request(extra, &json)?);
        }
        if read && unread && last != 0 {
            let purpose =
                RequestPurpose::Threads(ThreadsPurpose::ReadDirectMessagesChatTopic { topic_id });
            let extra = self.session.request(purpose, Some(chat_id));
            let json = view_messages(
                extra,
                chat_id,
                &[MessageId(last)],
                "messageSourceDirectMessagesChatTopicHistory",
                true,
            );
            sent = Some(self.send_json_request(extra, &json)?);
        }
        Ok(sent)
    }

    /// "Delete chat": every message of the conversation
    /// (`deleteDirectMessagesChatTopicHistory`).
    pub fn delete_direct_topic(
        &mut self,
        chat_id: ChatId,
        topic_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !self.session.is_direct_messages_admin(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose =
            RequestPurpose::Threads(ThreadsPurpose::DeleteDirectMessagesChatTopicHistory {
                topic_id,
            });
        let extra = self.session.request(purpose, Some(chat_id));
        let json = delete_direct_messages_chat_topic_history(extra, chat_id, topic_id);
        self.send_json_request(extra, &json)
    }

    /// Ask the channel whether the user behind a conversation is banned
    /// (tdesktop `kickedValue`), once per user.
    pub fn fetch_direct_member(
        &mut self,
        channel: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Groups(GroupsPurpose::GetDirectMessagesMember { user_id });
        if self
            .session
            .direct_member_banned(channel, user_id)
            .is_some()
            || self.session.requests.has_purpose_for_chat(purpose, channel)
        {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(channel));
        let json = get_chat_member(extra, channel, user_id);
        self.send_json_request(extra, &json).map(Some)
    }

    /// "Ban user from channel" / "Unban user in channel": the conversation's
    /// user in the broadcast channel (`setChatMemberStatus`).
    pub fn set_direct_member_banned(
        &mut self,
        channel: ChatId,
        user_id: i64,
        banned: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if banned {
            self.ban_chat_member(channel, user_id, 0)
        } else {
            self.remove_from_banned(channel, user_id)
        }
    }

    /// The message fee of one conversation: "Remove Fee" lets the user
    /// write for free (and may refund what they paid), "Charge Fee" asks
    /// them to pay again.
    pub fn set_direct_topic_free(
        &mut self,
        chat_id: ChatId,
        topic_id: i64,
        free: bool,
        refund: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !self.session.is_direct_messages_admin(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Threads(
            ThreadsPurpose::ToggleDirectMessagesChatTopicCanSendUnpaidMessages { topic_id },
        );
        let extra = self.session.request(purpose, Some(chat_id));
        let json = toggle_direct_messages_chat_topic_can_send_unpaid_messages(
            extra,
            chat_id,
            topic_id,
            free,
            free && refund,
        );
        self.send_json_request(extra, &json)
    }

    /// Stars the conversation has paid, asked when the Remove Fee box opens.
    pub fn fetch_direct_topic_revenue(
        &mut self,
        chat_id: ChatId,
        topic_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose =
            RequestPurpose::Threads(ThreadsPurpose::GetDirectMessagesChatTopicRevenue { topic_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let json = get_direct_messages_chat_topic_revenue(extra, chat_id, topic_id);
        self.send_json_request(extra, &json).map(Some)
    }
}
