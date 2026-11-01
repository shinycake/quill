//! Connect driver: subsection tabs (bots with topics, forums with tabs) —
//! the tab layout toggle and the tab context menu's topic actions.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::RequestPurpose;
use crate::state::ThreadsPurpose;
use crate::subsection_tabs::SubsectionTabsMode;
use crate::telegram::envelope::{
    ChatListPayload, ChatsPayload, GroupsPayload, MessagesPayload, ThreadsPayload, UsersPayload,
};
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
        self.save_media_prefs();
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
            // A chat entering a list (tdesktop preloads the topics of every
            // forum in a chat list).
            EnvelopePayload::ChatList(ChatListPayload::UpdateChatPosition(position)) => {
                Some(position.chat_id)
            }
            _ => None,
        }
    }

    /// A supergroup that just turned out to be a forum (`updateSupergroup`):
    /// the chats backed by it are checked after `apply`.
    pub(crate) fn possible_forum_supergroup(payload: &EnvelopePayload) -> Option<i64> {
        match payload {
            EnvelopePayload::Groups(GroupsPayload::UpdateSupergroup {
                supergroup_id,
                is_forum: true,
                ..
            }) => Some(*supergroup_id),
            _ => None,
        }
    }

    /// Load the topic list of a forum (or a bot with topics) as soon as it
    /// is known, so its chat row can list the titles before the chat is
    /// opened (tdesktop `Forum::preloadTopics`). The private chat id of a
    /// bot equals the bot's user id.
    pub(crate) fn maybe_preload_topics(&mut self, chat: Option<ChatId>, supergroup: Option<i64>) {
        // A supergroup's chat id is derived from its id (the bot API id).
        let from_supergroup = supergroup.map(|id| ChatId(-1_000_000_000_000 - id));
        for chat_id in chat.into_iter().chain(from_supergroup) {
            self.preload_topics(chat_id);
        }
    }

    /// Every forum already known, once the account is ready: updates that
    /// came before are asked for then.
    pub(crate) fn preload_known_forums(&mut self) {
        let ids: Vec<ChatId> = self.session.chats.keys().map(|id| ChatId(*id)).collect();
        for chat_id in ids {
            self.preload_topics(chat_id);
        }
    }

    /// Each forum is asked once, and only after a request went out (or one is
    /// already on its way). Not ready yet: it is asked later. A TDLib error
    /// is final (tdesktop does not retry either).
    fn preload_topics(&mut self, chat_id: ChatId) {
        // Only chats in a chat list (`history->inChatList()`): forums, and
        // bots with topics, whose topics also fill their row.
        let listed = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            (chat.is_forum_chat() || self.session.bot_topics(chat_id).is_some())
                && (chat.in_main_list || chat.in_archive || !chat.folder_positions.is_empty())
        });
        if !listed
            || !self.chats_path_active()
            || self.session.threads.forum_row_asked.contains(&chat_id.0)
        {
            return;
        }
        if self.maybe_fetch_forum_topics(chat_id).is_ok() {
            self.session.threads.forum_row_asked.insert(chat_id.0);
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
        let Some((settings, previous)) =
            self.session
                .threads
                .forum_topics
                .update_topic(chat_id.0, forum_topic_id, |topic| {
                    let settings = topic.notification_settings.clone().with_mute_for(if muted {
                        MUTE_FOREVER
                    } else {
                        0
                    });
                    let previous =
                        std::mem::replace(&mut topic.notification_settings, settings.clone());
                    (settings, previous)
                })
        else {
            return Ok(None);
        };
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
            self.session
                .threads
                .forum_topics
                .update_topic(chat_id.0, forum_topic_id, |topic| {
                    topic.notification_settings = previous;
                });
            return Err(err);
        }
        Ok(Some(extra))
    }
}
