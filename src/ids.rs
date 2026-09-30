use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Local account/session namespace. One active account in this phase.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountKey(pub String);

impl AccountKey {
    pub fn primary() -> Self {
        Self("primary".to_string())
    }

    /// Filesystem-safe ids only: non-empty, ASCII alphanumerics plus `-`
    /// and `_`. This keeps `accounts/<key>` paths free of separators and
    /// traversal (`.` is not allowed, so `..` cannot occur).
    pub fn new(id: &str) -> Option<Self> {
        if id.is_empty() {
            return None;
        }
        if id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            Some(Self(id.to_string()))
        } else {
            None
        }
    }
}

impl fmt::Display for AccountKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// TDLib `int53` chat identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ChatId(pub i64);

/// TDLib `int53` message identifier. Unique only within a chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct MessageId(pub i64);

/// TDLib `int53` user identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub i64);

/// TDLib `int32` file identifier (`file.id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub i32);

/// Cross-chat identity: a message ID alone is not enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageKey {
    pub chat_id: ChatId,
    pub message_id: MessageId,
}

impl MessageKey {
    pub fn new(chat_id: ChatId, message_id: MessageId) -> Self {
        Self {
            chat_id,
            message_id,
        }
    }
}

/// Typed topic identity. Numeric thread IDs are not interchangeable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TopicId {
    None,
    Forum { forum_topic_id: i64 },
    DirectMessages { direct_messages_chat_topic_id: i64 },
    SavedMessages { saved_messages_topic_id: i64 },
}

/// Conversation identity including optional topic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConversationKey {
    pub account: AccountKey,
    pub chat_id: ChatId,
    pub topic: TopicId,
}

/// Monotonic request correlation encoded in `@extra` as a decimal string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);

impl RequestId {
    pub fn as_extra(self) -> String {
        self.0.to_string()
    }
}

impl FromStr for RequestId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>().map(RequestId)
    }
}

/// Increments when the viewed chat/topic/account/search context changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ViewGeneration(pub u64);

impl ViewGeneration {
    pub fn bump(&mut self) {
        self.0 = self.0.saturating_add(1);
    }
}

/// Increments on logout / new client so in-flight work cannot apply to a new session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AccountGeneration(pub u64);

impl AccountGeneration {
    pub fn bump(&mut self) {
        self.0 = self.0.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_key_new_accepts_safe_ids() {
        for id in ["primary", "account-1", "work_2", "a", "A9-_z"] {
            assert_eq!(AccountKey::new(id), Some(AccountKey(id.to_string())));
        }
    }

    #[test]
    fn account_key_new_rejects_unsafe_ids() {
        // Empty, traversal, separators, spaces, and dots all rejected —
        // the id becomes a directory name under `accounts/`.
        for id in [
            "", "..", "../x", "a/b", "a\\b", "a b", "a.b", ".hidden", "é",
        ] {
            assert_eq!(AccountKey::new(id), None, "id {id:?} must be rejected");
        }
    }
}
