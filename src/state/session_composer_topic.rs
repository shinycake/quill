//! The topic the compose field's placeholder names (tdesktop
//! `HistoryWidget::resolveReplyToTopic`): "Message in {topic}", or "Reply in
//! {topic}" while a reply is pending.
use super::*;
use crate::composer::TopicPlaceholder;

/// `Data::ForumTopic::kGeneralId`.
const GENERAL_TOPIC_ID: i32 = 1;

impl Session {
    /// The topic a message sent from the compose field of `chat_id` goes to,
    /// in a forum supergroup: the topic of the message being replied to, else
    /// the open topic, else General (where a send without either lands).
    /// `None` outside a forum supergroup, and in a reply thread, which the
    /// message goes to instead of a topic.
    pub fn composer_topic(
        &self,
        chat_id: ChatId,
        reply_to: Option<MessageId>,
    ) -> Option<TopicPlaceholder> {
        if !self.chats.get(&chat_id.0)?.is_forum_chat() || self.thread_for_chat(chat_id).is_some() {
            return None;
        }
        let topic_id = reply_to
            .and_then(|message| self.topic_of_message(chat_id, message))
            .or(self.open_topic);
        let topics = self.threads.forum_topics.get(&chat_id.0);
        let known = topics.and_then(|topics| {
            topics.iter().find(|topic| match topic_id {
                Some(id) => topic.forum_topic_id == id,
                None => topic.is_general,
            })
        });
        let general = topic_id.is_none_or(|id| id == GENERAL_TOPIC_ID)
            || known.is_some_and(|topic| topic.is_general);
        let title = known
            .map(|topic| topic.name.trim())
            .filter(|name| !name.is_empty())
            .map_or_else(
                || if general { "General" } else { "Topic" }.to_string(),
                str::to_string,
            );
        Some(TopicPlaceholder {
            title,
            reply: reply_to.is_some(),
        })
    }

    /// The loaded topic whose history holds `message_id`: the open topic's
    /// first, then the others.
    fn topic_of_message(&self, chat_id: ChatId, message_id: MessageId) -> Option<i32> {
        let holds = |topic: i32| {
            self.threads
                .topic_histories
                .get(&(chat_id.0, topic))
                .is_some_and(|history| history.messages.get(&message_id.0).is_some())
        };
        if let Some(open) = self.open_topic.filter(|&open| holds(open)) {
            return Some(open);
        }
        self.threads
            .topic_histories
            .iter()
            .find(|((chat, _), history)| {
                *chat == chat_id.0 && history.messages.get(&message_id.0).is_some()
            })
            .map(|((_, topic), _)| *topic)
    }

    /// Whether the chat is a bot that makes its own topics, with none open:
    /// the field asks for an "Off-thread message".
    pub fn bot_chat_off_thread(&self, chat_id: ChatId) -> bool {
        self.open_topic.is_none()
            && self
                .bot_topics(chat_id)
                .is_some_and(|bot| !bot.allows_users_to_create_topics)
    }
}
