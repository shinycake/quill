//! A channel's direct messages as its admins see them (tdesktop monoforum
//! admin): the conversations, the one on screen, and the composer's
//! "Choose a message to reply." lock in the All view.
//!
//! The chat's main history shows the open conversation: picking one swaps
//! the window and every page comes from `getDirectMessagesChatTopicHistory`,
//! so the reader, unread bar and jump-down keep working unchanged.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::composer::DraftTopic;
use crate::suggested_posts::DirectMessagesRole;
use crate::telegram::envelope::DirectMessagesTopic;

/// How many conversations one `loadDirectMessagesChatTopics` asks for.
pub const DIRECT_TOPICS_PAGE: i32 = 50;

impl Session {
    /// The topic a send from the composer of `chat_id` goes to: the open
    /// conversation of a channel's direct messages, or the open forum topic.
    pub fn send_topic(&self, chat_id: ChatId) -> Option<crate::ids::TopicId> {
        if let Some(topic) = self.open_direct_topic(chat_id) {
            return Some(crate::ids::TopicId::DirectMessages {
                direct_messages_chat_topic_id: topic,
            });
        }
        if self.open_chat == Some(chat_id) {
            self.open_topic.map(crate::ids::TopicId::forum)
        } else {
            None
        }
    }

    pub fn is_direct_messages_admin(&self, chat_id: ChatId) -> bool {
        self.direct_messages_role(chat_id) == Some(DirectMessagesRole::Admin)
    }

    /// The conversation an admin has open in `chat_id`; `None` = All.
    pub fn open_direct_topic(&self, chat_id: ChatId) -> Option<i64> {
        if !self.is_direct_messages_admin(chat_id) {
            return None;
        }
        self.threads.direct.open_topic(chat_id.0)
    }

    pub fn direct_topic(&self, chat_id: ChatId, topic_id: i64) -> Option<&DirectMessagesTopic> {
        self.threads.direct.get(chat_id.0, topic_id)
    }

    pub fn open_direct_topic_info(&self, chat_id: ChatId) -> Option<&DirectMessagesTopic> {
        self.direct_topic(chat_id, self.open_direct_topic(chat_id)?)
    }

    /// The channel turned its direct messages off: the group's full info
    /// no longer names a channel (tdesktop `monoforumDisabled`, set when
    /// the monoforum loses its link). Unknown full info is not "off".
    pub fn direct_messages_disabled(&self, chat_id: ChatId) -> bool {
        if self.direct_messages_role(chat_id).is_none() {
            return false;
        }
        self.chat_supergroup(chat_id)
            .and_then(|id| self.groups.supergroup_full_infos.get(&id))
            .is_some_and(|info| info.direct_messages_chat_id == 0)
    }

    /// `updateDirectMessagesChatTopic` / `directMessagesChatTopic`.
    pub(crate) fn apply_direct_topic(&mut self, mut topic: DirectMessagesTopic) {
        let chat_id = ChatId(topic.chat_id);
        let slot = DraftTopic::DirectMessages(topic.id);
        let draft = topic.draft.take();
        topic.draft = self.draft_for(chat_id, slot).cloned();
        self.threads
            .direct
            .topics
            .entry(topic.chat_id)
            .or_default()
            .insert(topic.id, topic);
        self.set_remote_draft(chat_id, slot, draft);
        self.view_generation.bump();
    }

    /// Show `topic` (or All, for `None`) in `chat_id`. Swaps the history
    /// window: in-flight pages for the old view are dropped on arrival.
    /// Returns whether anything changed.
    pub fn select_direct_topic(&mut self, chat_id: ChatId, topic: Option<i64>) -> bool {
        if !self.is_direct_messages_admin(chat_id) || self.open_direct_topic(chat_id) == topic {
            return false;
        }
        match topic {
            Some(topic) => {
                self.threads.direct.open.insert(chat_id.0, topic);
            }
            None => {
                self.threads.direct.open.remove(&chat_id.0);
            }
        }
        self.reset_history_window(chat_id);
        self.history_entry(chat_id).loaded_complete = false;
        self.prepare_history_view(chat_id);
        self.view_generation.bump();
        true
    }

    /// Where the open conversation's reader left off, like
    /// [`ChatSummary::unread_anchor`] for a chat. `None` when no
    /// conversation is open.
    pub(crate) fn direct_topic_unread_anchor(&self, chat_id: ChatId) -> Option<Option<MessageId>> {
        let topic = self.open_direct_topic_info(chat_id)?;
        let last_read = topic.last_read_inbox_message_id;
        Some(
            (topic.unread_count > 0 && last_read > 0 && topic.last_message_id > last_read)
                .then_some(MessageId(last_read)),
        )
    }

    /// The newest message of the open conversation (the window's
    /// "has newer" edge); `None` when no conversation is open.
    pub(crate) fn direct_topic_latest_id(&self, chat_id: ChatId) -> Option<i64> {
        self.open_direct_topic(chat_id)?;
        Some(
            self.open_direct_topic_info(chat_id)
                .map_or(0, |topic| topic.last_message_id),
        )
    }

    pub(crate) fn outside_open_direct_topic(&self, chat_id: ChatId, direct_topic_id: i64) -> bool {
        self.open_direct_topic(chat_id)
            .is_some_and(|open| open != direct_topic_id)
    }

    pub fn direct_topic_name(&self, topic: &DirectMessagesTopic) -> String {
        self.sender_name_and_accent(Some(topic.sender))
            .0
            .unwrap_or_default()
    }

    /// The short name tabs show (tdesktop `shortName`).
    pub fn direct_topic_short_name(&self, topic: &DirectMessagesTopic) -> String {
        self.sender_first_name(topic.sender)
    }

    /// The conversation a loaded message of `chat_id` belongs to.
    pub fn direct_topic_of_message(&self, chat_id: ChatId, message_id: MessageId) -> Option<i64> {
        self.histories
            .get(&chat_id.0)?
            .messages
            .get(&message_id.0)
            .map(|message| message.extras.direct_messages_topic_id)
            .filter(|topic| *topic != 0)
    }

    /// tdesktop `HistoryWidget::updateCanSendMessage` for a monoforum admin
    /// in the All view: the field is locked behind "Choose a message to
    /// reply." until a reply to one of the chat's conversations is picked.
    /// Not while the channel's direct messages are off (no one can send
    /// then, and the usual restriction says so).
    pub fn direct_reply_locked(&self, chat_id: ChatId, replying_to: Option<MessageId>) -> bool {
        self.is_direct_messages_admin(chat_id)
            && self.open_direct_topic(chat_id).is_none()
            && !self.direct_messages_disabled(chat_id)
            && replying_to
                .and_then(|message| self.direct_topic_of_message(chat_id, message))
                .is_none()
    }

    /// `messages.total_count` of a page of the open conversation: the
    /// header's "N messages".
    pub(crate) fn note_direct_topic_count(
        &mut self,
        pending: Option<&PendingRequest>,
        total_count: i32,
    ) {
        let Some(pending) = pending.filter(|p| {
            matches!(
                p.purpose,
                RequestPurpose::GetHistory
                    | RequestPurpose::GetHistoryNewer
                    | RequestPurpose::GetHistoryAround
            )
        }) else {
            return;
        };
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        if self.stale_history_requests.contains(&pending.id.0) {
            return;
        }
        if let Some(topic) = self.open_direct_topic(chat_id) {
            self.threads
                .direct
                .message_counts
                .insert((chat_id.0, topic), total_count.max(0));
        }
    }

    /// "N messages" of the open conversation once a page told.
    pub fn direct_topic_message_count(&self, chat_id: ChatId) -> Option<i32> {
        let topic = self.open_direct_topic(chat_id)?;
        self.threads
            .direct
            .message_counts
            .get(&(chat_id.0, topic))
            .copied()
    }

    /// The price of a message to the channel (`supergroup.paid_message_star_count`
    /// of the direct messages group); 0 when free.
    pub fn direct_messages_price(&self, chat_id: ChatId) -> i64 {
        self.chat_supergroup(chat_id)
            .and_then(|id| self.payments.paid_messages.dm_price.get(&id).copied())
            .unwrap_or(0)
    }

    pub fn direct_topic_count(&self, chat_id: ChatId) -> usize {
        self.threads.direct.ordered(chat_id.0).len()
    }

    /// The jump-down counter of an admin's direct messages: the open
    /// conversation's unread messages, or in All every conversation's
    /// (tdesktop `chatListUnreadState().messages`). `None` for any other
    /// chat.
    pub fn direct_unread_count(&self, chat_id: ChatId) -> Option<i32> {
        if !self.is_direct_messages_admin(chat_id) {
            return None;
        }
        Some(match self.open_direct_topic_info(chat_id) {
            Some(topic) => topic.unread_count,
            None => self.threads.direct.unread_messages(chat_id.0),
        })
    }

    /// Whether the user behind a conversation is banned from the channel;
    /// `None` until asked.
    pub fn direct_member_banned(&self, channel: ChatId, user_id: i64) -> Option<bool> {
        self.threads
            .direct
            .kicked
            .get(&(channel.0, user_id))
            .copied()
    }

    pub fn set_direct_member_banned(&mut self, channel: ChatId, user_id: i64, banned: bool) {
        self.threads
            .direct
            .kicked
            .insert((channel.0, user_id), banned);
        self.view_generation.bump();
    }

    /// Stars this conversation paid; `None` until asked.
    pub fn direct_topic_revenue(&self, chat_id: ChatId, topic_id: i64) -> Option<i64> {
        self.threads
            .direct
            .revenue
            .get(&(chat_id.0, topic_id))
            .copied()
    }

    pub(crate) fn set_direct_topic_revenue(&mut self, chat_id: ChatId, topic_id: i64, stars: i64) {
        self.threads
            .direct
            .revenue
            .insert((chat_id.0, topic_id), stars.max(0));
        self.view_generation.bump();
    }

    /// A deleted conversation leaves the list at once (TDLib then sends it
    /// with order 0); the chat falls back to All if it was open.
    pub(crate) fn forget_direct_topic(&mut self, chat_id: ChatId, topic_id: i64) {
        if let Some(topics) = self.threads.direct.topics.get_mut(&chat_id.0) {
            topics.remove(&topic_id);
        }
        // What was told about the conversation goes with it, so a topic id
        // that comes back does not show the old count or revenue. (The
        // banned flag is the user's in the channel, not the conversation's.)
        let key = (chat_id.0, topic_id);
        self.threads.direct.message_counts.remove(&key);
        self.threads.direct.revenue.remove(&key);
        if self.open_direct_topic(chat_id) == Some(topic_id) {
            self.select_direct_topic(chat_id, None);
        }
        self.view_generation.bump();
    }

    pub(crate) fn settle_direct_messages_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(pending) = pending else {
            return;
        };
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        match pending.purpose {
            RequestPurpose::Threads(ThreadsPurpose::DeleteDirectMessagesChatTopicHistory {
                topic_id,
            }) => self.forget_direct_topic(chat_id, topic_id),
            RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus { user_id, kind })
                if self.direct_member_banned(chat_id, user_id).is_some() =>
            {
                match kind {
                    MemberStatusChange::Ban => {
                        self.set_direct_member_banned(chat_id, user_id, true)
                    }
                    MemberStatusChange::Unban => {
                        self.set_direct_member_banned(chat_id, user_id, false)
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
