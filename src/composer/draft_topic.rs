//! Where a composer draft lives inside a chat.
//!
//! Telegram Desktop keys drafts by `(history, topicRootId, monoforumPeerId)`
//! (`Data::DraftKey`, data/data_drafts.h): the chat itself, a forum topic,
//! or a reply thread whose root is `topicRootId`
//! (`ChatWidget::resolvedTopicRootId` returns `_repliesRootId` for a
//! comments / replies view, forum or not). TDLib exposes the same split as
//! `setChatDraftMessage.topic_id: MessageTopic` (schema 1.8.68, line 13887).

/// The topic half of a draft key. `Chat` is TDLib's `topic_id: null`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DraftTopic {
    /// The chat's own draft (`chat.draft_message`, `updateChatDraftMessage`).
    #[default]
    Chat,
    /// A forum topic (`messageTopicForum`, line 3314): `forumTopic.draft_message`
    /// and `updateForumTopic.draft_message`.
    Forum(i32),
    /// A comment / reply thread in a non-forum chat (`messageTopicThread`,
    /// line 3311): `messageThreadInfo.draft_message`. TDLib keeps it on the
    /// thread root (`Message::thread_draft_message`); there is no update
    /// for it, `getMessageThread` returns the current value.
    Thread(i64),
}

impl DraftTopic {
    /// The `(chat_id, topic)` map key the reducer uses for dirty tracking.
    pub fn key(self, chat_id: i64) -> DraftKey {
        DraftKey {
            chat_id,
            topic: self,
        }
    }
}

/// One draft slot: a chat plus the topic inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DraftKey {
    pub chat_id: i64,
    pub topic: DraftTopic,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_carries_chat_and_topic() {
        let key = DraftTopic::Forum(3).key(11);
        assert_eq!(key.chat_id, 11);
        assert_eq!(key.topic, DraftTopic::Forum(3));
        assert_ne!(DraftTopic::Chat.key(11), DraftTopic::Thread(3).key(11));
        assert_eq!(DraftTopic::default(), DraftTopic::Chat);
    }
}
