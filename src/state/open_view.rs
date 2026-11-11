//! What the main window shows: nothing, a chat, or a topic of a forum chat.
//! One value, so a topic cannot outlive its chat and leaving the chat
//! leaves the topic with it.
use super::*;

/// The main window's view, owned by the account's [`Session`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OpenView {
    /// No chat is open.
    #[default]
    Closed,
    /// A chat is open: its own history, or a forum's topic list.
    Chat { chat: ChatId },
    /// A topic of the open forum chat.
    Topic { chat: ChatId, topic: i32 },
}

impl OpenView {
    /// A chat open (or none): the view without a topic.
    pub fn of(chat: Option<ChatId>) -> Self {
        chat.map_or(Self::Closed, |chat| Self::Chat { chat })
    }

    /// The same chat, showing `topic` (no chat open: still closed).
    pub fn with_topic(self, topic: i32) -> Self {
        self.chat()
            .map_or(Self::Closed, |chat| Self::Topic { chat, topic })
    }

    pub fn chat(self) -> Option<ChatId> {
        match self {
            Self::Closed => None,
            Self::Chat { chat } | Self::Topic { chat, .. } => Some(chat),
        }
    }

    pub fn topic(self) -> Option<i32> {
        match self {
            Self::Topic { topic, .. } => Some(topic),
            _ => None,
        }
    }
}

impl Session {
    /// The chat the main window shows.
    pub fn open_chat_id(&self) -> Option<ChatId> {
        self.open.chat()
    }

    /// The forum topic the main window shows, if any.
    pub fn open_forum_topic(&self) -> Option<i32> {
        self.open.topic()
    }
}
