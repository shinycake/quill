//! Connect driver: subsection tabs (bots with topics, forums with tabs) —
//! the tab layout toggle and the tab context menu's topic actions.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::RequestPurpose;
use crate::state::ThreadsPurpose;
use crate::subsection_tabs::SubsectionTabsMode;
use crate::telegram::envelope::{ChatsPayload, MessagesPayload, ThreadsPayload, UsersPayload};
use crate::telegram::envelope::{EnvelopePayload, MUTE_FOREVER};
use crate::telegram::requests::{
    get_forum_topic, set_forum_topic_notification_settings, view_messages,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// The tabs' toggle button: Top → Bottom → Left → Top, saved per chat
    /// with the account's media prefs (`subsectionTabsMode(peerId)` +
    /// `saveSettingsDelayed` in Telegram Desktop).
    pub fn cycle_subsection_tabs_mode(&mut self, chat_id: ChatId) -> SubsectionTabsMode {
        let mode = self.session.cycle_subsection_tabs_mode(chat_id);
        if self.save_media_prefs().is_err() {
            self.session.chats_state.chat_action_error =
                Some("Could not save the tab layout.".into());
        }
        mode
    }

    /// A bot that just turned out to have topics (`updateUser`) or a new
    /// chat (`updateNewChat`): the chat to check after `apply`.
    pub(crate) fn possible_topic_chat(payload: &EnvelopePayload) -> Option<ChatId> {
        match payload {
            EnvelopePayload::Users(UsersPayload::UpdateUser { user_id, user })
                if user.has_topics =>
            {
                Some(ChatId(user_id.0))
            }
            EnvelopePayload::Chats(ChatsPayload::UpdateNewChat { chat_id, .. }) => Some(*chat_id),
            _ => None,
        }
    }

    /// Load a bot chat's topic list once it is known to have topics, so the
    /// chat row can show the topic names, as Telegram Desktop's forum rows
    /// do. The private chat id equals the bot's user id.
    pub(crate) fn maybe_fetch_bot_topics(&mut self, chat_id: Option<ChatId>) {
        if let Some(chat_id) = chat_id
            && self.session.bot_topics(chat_id).is_some()
        {
            let _ = self.maybe_fetch_forum_topics(chat_id);
        }
    }

    /// A topic whose TDLib state may have changed: its read position or
    /// settings (`updateForumTopic`), its info (`updateForumTopicInfo`, e.g.
    /// a new topic) or a new message in it. Checked before `apply`.
    pub(crate) fn possible_topic_refresh(payload: &EnvelopePayload) -> Option<(ChatId, i32)> {
        match payload {
            EnvelopePayload::Threads(ThreadsPayload::UpdateForumTopic(update)) => {
                Some((ChatId(update.chat_id), update.forum_topic_id))
            }
            EnvelopePayload::Threads(ThreadsPayload::UpdateForumTopicInfo(info)) => {
                Some((ChatId(info.chat_id), info.forum_topic_id))
            }
            EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
                message.topic_id.map(|topic| (message.chat_id, topic))
            }
            _ => None,
        }
    }

    /// `getForumTopic` for one topic of a loaded topic list, so its unread
    /// count and read position come from TDLib (deduped while in flight).
    pub(crate) fn maybe_refresh_forum_topic(&mut self, target: Option<(ChatId, i32)>) {
        let Some((chat_id, forum_topic_id)) = target else {
            return;
        };
        if !self.chats_path_active() || !self.session.threads.forum_topics.contains_key(&chat_id.0)
        {
            return;
        }
        let purpose = RequestPurpose::Threads(ThreadsPurpose::GetForumTopic { forum_topic_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return;
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if self
            .sender
            .send_json(&get_forum_topic(extra, chat_id, forum_topic_id))
            .is_err()
        {
            self.session.requests.take(extra);
        }
    }

    /// Tab menu "Mark as read": `viewMessages` on the topic's last message
    /// with `messageSourceForumTopicHistory` and `force_read`.
    pub fn mark_forum_topic_read(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_has_topics(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(last) = self
            .session
            .threads
            .forum_topics
            .get(&chat_id.0)
            .and_then(|topics| topics.iter().find(|t| t.forum_topic_id == forum_topic_id))
            .map(|topic| topic.last_message_id)
            .filter(|id| *id != 0)
        else {
            return Ok(None);
        };
        let purpose = RequestPurpose::Threads(ThreadsPurpose::ReadForumTopic { forum_topic_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&view_messages(
            extra,
            chat_id,
            &[MessageId(last)],
            "messageSourceForumTopicHistory",
            true,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session
            .mark_forum_topic_read_locally(chat_id, forum_topic_id);
        Ok(Some(extra))
    }

    /// Tab menu Mute / Unmute: `setForumTopicNotificationSettings` with the
    /// topic's current settings and only `mute_for` changed.
    pub fn set_forum_topic_muted(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        muted: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_has_topics(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(topic) = self
            .session
            .threads
            .forum_topics
            .get_mut(&chat_id.0)
            .and_then(|topics| {
                topics
                    .iter_mut()
                    .find(|t| t.forum_topic_id == forum_topic_id)
            })
        else {
            return Ok(None);
        };
        let settings =
            topic
                .notification_settings
                .clone()
                .with_mute_for(if muted { MUTE_FOREVER } else { 0 });
        let previous = std::mem::replace(&mut topic.notification_settings, settings.clone());
        let purpose = RequestPurpose::Threads(ThreadsPurpose::SetForumTopicNotificationSettings {
            forum_topic_id,
        });
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_forum_topic_notification_settings(
                extra,
                chat_id,
                forum_topic_id,
                &settings,
            ))
        {
            self.session.requests.take(extra);
            if let Some(topic) = self
                .session
                .threads
                .forum_topics
                .get_mut(&chat_id.0)
                .and_then(|t| t.iter_mut().find(|t| t.forum_topic_id == forum_topic_id))
            {
                topic.notification_settings = previous;
            }
            return Err(err);
        }
        Ok(Some(extra))
    }
}
