//! What an id-less TDLib answer is about. An `ok`, a `secretChat`, a
//! `userFullInfo` carries nothing that says which user, folder or story the
//! request was for, so the request records it. A request has one key, set
//! by the sender that knows it (`Session::request_for_*`), never a set of
//! optional fields stamped one by one.
use super::*;

/// The thing one request acts on, besides its chat.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RequestKey {
    /// The answer names what it is for, or the purpose says it all.
    #[default]
    None,
    /// The file a `downloadFile` fetches.
    File(i32),
    /// The message a history page is asked around, or a message-scoped
    /// answer is for.
    Message(MessageId),
    /// The forum topic of a `GetTopicHistory` page.
    Topic(i32),
    /// The user of a user-scoped request.
    User(i64),
    Supergroup(i64),
    Community(i64),
    /// The story of a `GetStory`.
    Story(i32),
    /// The story ids a `SetChatPinnedStories` sent, or the album order a
    /// `ReorderStoryAlbums` sent.
    StoryList(Vec<i32>),
    /// The album a `DeleteStoryAlbum` drops.
    StoryAlbum(i32),
    /// The user ids a `SetCloseFriends` sent.
    UserIds(Vec<i64>),
    Folder(i32),
    /// The notification settings scope.
    Scope(NotificationSettingsScope),
    SecretChat(i32),
    /// The invite link a link-scoped request acts on.
    InviteLink(String),
    /// The string id of what an `ok` is for: the terms an
    /// `acceptTermsOfService` accepts, the gift a `sellGift` sells.
    Subject(String),
}

impl PendingRequest {
    pub fn file_id(&self) -> Option<i32> {
        match self.key {
            RequestKey::File(id) => Some(id),
            _ => None,
        }
    }

    pub fn around_message_id(&self) -> Option<MessageId> {
        match self.key {
            RequestKey::Message(id) => Some(id),
            _ => None,
        }
    }

    pub fn forum_topic_id(&self) -> Option<i32> {
        match self.key {
            RequestKey::Topic(id) => Some(id),
            _ => None,
        }
    }

    pub fn user_id(&self) -> Option<i64> {
        match self.key {
            RequestKey::User(id) => Some(id),
            _ => None,
        }
    }

    pub fn supergroup_id(&self) -> Option<i64> {
        match self.key {
            RequestKey::Supergroup(id) => Some(id),
            _ => None,
        }
    }

    pub fn community_id(&self) -> Option<i64> {
        match self.key {
            RequestKey::Community(id) => Some(id),
            _ => None,
        }
    }

    pub fn story_id(&self) -> Option<i32> {
        match self.key {
            RequestKey::Story(id) => Some(id),
            _ => None,
        }
    }

    pub fn story_ids(&self) -> Option<&[i32]> {
        match &self.key {
            RequestKey::StoryList(ids) => Some(ids),
            _ => None,
        }
    }

    pub fn story_album_id(&self) -> Option<i32> {
        match self.key {
            RequestKey::StoryAlbum(id) => Some(id),
            _ => None,
        }
    }

    pub fn user_ids(&self) -> Option<&[i64]> {
        match &self.key {
            RequestKey::UserIds(ids) => Some(ids),
            _ => None,
        }
    }

    pub fn folder_id(&self) -> Option<i32> {
        match self.key {
            RequestKey::Folder(id) => Some(id),
            _ => None,
        }
    }

    pub fn scope(&self) -> Option<NotificationSettingsScope> {
        match self.key {
            RequestKey::Scope(scope) => Some(scope),
            _ => None,
        }
    }

    pub fn secret_chat_id(&self) -> Option<i32> {
        match self.key {
            RequestKey::SecretChat(id) => Some(id),
            _ => None,
        }
    }

    pub fn invite_link(&self) -> Option<&str> {
        match &self.key {
            RequestKey::InviteLink(link) => Some(link),
            _ => None,
        }
    }

    pub fn subject_id(&self) -> Option<&str> {
        match &self.key {
            RequestKey::Subject(id) => Some(id),
            _ => None,
        }
    }
}
