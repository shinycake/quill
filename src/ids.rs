#![deny(clippy::indexing_slicing)]

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

impl ChatId {
    /// The chat TDLib gives a supergroup or channel
    /// (`DialogId(ChannelId)`: `-1000000000000 - channel_id`).
    pub fn from_supergroup(supergroup_id: i64) -> Self {
        Self((-1_000_000_000_000_i64).saturating_sub(supergroup_id))
    }
}

/// TDLib `int53` message identifier. Unique only within a chat.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord, Default,
)]
pub struct MessageId(pub i64);

/// Something of one account: a bot, a custom emoji, a file. Sets of "asked
/// once" ids live app-wide, but TDLib answers per account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OfAccount<T> {
    pub account: AccountKey,
    pub item: T,
}

impl<T> OfAccount<T> {
    pub fn new(account: AccountKey, item: T) -> Self {
        Self { account, item }
    }
}

/// An app-wide value that belongs to one account at a time: kept by ids
/// that repeat across accounts (chat and message ids), so the account it
/// was filled for is part of its key. Asked for under another account it
/// starts over, whatever path changed the account on screen.
#[derive(Debug, Default)]
pub struct OwnedBy<T> {
    owner: Option<AccountKey>,
    value: T,
}

impl<T: Default> OwnedBy<T> {
    /// The value for `owner`: the one kept when it is `owner`'s, else a
    /// fresh one (what another account kept is dropped).
    pub fn for_owner(&mut self, owner: &AccountKey) -> &mut T {
        if self.owner.as_ref() != Some(owner) {
            self.owner = Some(owner.clone());
            self.value = T::default();
        }
        &mut self.value
    }

    /// The account the value was filled for.
    pub fn owner(&self) -> Option<&AccountKey> {
        self.owner.as_ref()
    }

    /// The value, for reading (and for changes that stay with whatever
    /// owner it has: forgetting entries, clearing).
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> std::ops::Deref for OwnedBy<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

/// A chat of one account: chat ids are TDLib's, the same for a group two
/// signed-in accounts share, so per-chat memory kept app-wide carries the
/// account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountChat {
    pub account: AccountKey,
    pub chat: ChatId,
}

impl AccountChat {
    pub fn new(account: AccountKey, chat: ChatId) -> Self {
        Self { account, chat }
    }
}

/// A message of one chat. Message ids repeat across chats (supergroups and
/// channels number their own), so state kept per message is keyed by both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChatMessage {
    pub chat: ChatId,
    pub message: MessageId,
}

impl ChatMessage {
    pub fn new(chat: ChatId, message: MessageId) -> Self {
        Self { chat, message }
    }
}

impl MessageId {
    /// Convert a Telegram server message ID (as used by deep links) to TDLib's
    /// client ID. TDLib reserves 20 low bits for local IDs and type flags:
    /// <https://github.com/tdlib/td/blob/master/td/telegram/MessageId.h>
    /// (`SERVER_ID_SHIFT` and the `ServerMessageId` constructor).
    pub fn from_server_id(server_id: i64) -> Self {
        Self(server_id << 20)
    }
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopicId {
    None,
    Forum { forum_topic_id: i64 },
    DirectMessages { direct_messages_chat_topic_id: i64 },
    SavedMessages { saved_messages_topic_id: i64 },
}

impl TopicId {
    pub fn forum(forum_topic_id: i32) -> Self {
        Self::Forum {
            forum_topic_id: i64::from(forum_topic_id),
        }
    }

    pub fn forum_topic_id(self) -> Option<i32> {
        match self {
            Self::Forum { forum_topic_id } => i32::try_from(forum_topic_id).ok(),
            _ => None,
        }
    }
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
    fn a_supergroup_chat_id_follows_tdlib() {
        assert_eq!(
            ChatId::from_supergroup(1_234_567_890),
            ChatId(-1_001_234_567_890)
        );
        assert_eq!(ChatId::from_supergroup(1), ChatId(-1_000_000_000_001));
    }

    #[test]
    fn message_id_from_server_id_scales() {
        assert_eq!(MessageId::from_server_id(42).0, 42 << 20);
        assert_eq!(MessageId::from_server_id(1).0, 1_048_576);
    }

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
