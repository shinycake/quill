//! What an answer needs to know beyond the request's purpose and chat: the
//! one thing the request was about. One value per request, set when it is
//! registered, so an answer reads exactly what its builder wrote.
use super::*;

/// The subject a pending request carries for its answer (an `ok` or an
/// id-less object names none of its own).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Correlation {
    /// The purpose and chat say it all.
    #[default]
    None,
    /// A history page: the epoch of the chat's window it was asked for
    /// ([`HistoryState::window_epoch`]; an answer for another epoch asks about
    /// a window that has since been replaced) and the `from_message_id` it
    /// started at, when it has one.
    HistoryPage {
        window_epoch: u64,
        from: Option<MessageId>,
    },
    /// The message a request acts on (a game score, a mini-app launch).
    Message(MessageId),
    /// `getTopicHistory`: the forum topic.
    ForumTopic(i32),
    User(i64),
    Supergroup(i64),
    Community(i64),
    Story(i32),
    /// An album request whose `ok` carries no payload: the sent story ids
    /// (`SetChatPinnedStories` the new pinned list, `ReorderStoryAlbums` the
    /// new order) or the album a delete named.
    StoryAlbum {
        story_ids: Option<Vec<i32>>,
        album_id: Option<i32>,
    },
    /// `SetCloseFriends`: the sent list.
    UserIds(Vec<i64>),
    Folder(i32),
    Scope(NotificationSettingsScope),
    SecretChat(i32),
    /// The invite link a link-scoped request acts on.
    InviteLink(String),
    /// The string id of the thing an id-less `ok` is for (the terms
    /// `acceptTermsOfService` accepts, the gift `sellGift` sells).
    Subject(String),
    /// `downloadFile`: the file.
    File(i32),
    /// A page the shared-media gallery's Stories tab asked for (the stories
    /// page asks for the same purpose without it).
    GalleryTab,
    /// A page of a paged list (the archived or trending sticker sets): the
    /// offset it started at (0 = the first page, which replaces the list).
    PageFrom(i64),
}

impl PendingRequest {
    /// The offset a page was asked from; 0 for the first page.
    pub fn page_from(&self) -> i64 {
        match &self.correlation {
            Correlation::PageFrom(from) => *from,
            _ => 0,
        }
    }

    /// `from_message_id` of a history page, or the message a request acts on.
    pub fn around_message_id(&self) -> Option<MessageId> {
        match &self.correlation {
            Correlation::HistoryPage { from, .. } => *from,
            Correlation::Message(id) => Some(*id),
            _ => None,
        }
    }

    /// The window epoch a history page was asked for.
    pub fn window_epoch(&self) -> Option<u64> {
        match &self.correlation {
            Correlation::HistoryPage { window_epoch, .. } => Some(*window_epoch),
            _ => None,
        }
    }

    pub fn forum_topic_id(&self) -> Option<i32> {
        match &self.correlation {
            Correlation::ForumTopic(topic) => Some(*topic),
            _ => None,
        }
    }

    pub fn user_id(&self) -> Option<i64> {
        match &self.correlation {
            Correlation::User(user) => Some(*user),
            _ => None,
        }
    }

    pub fn supergroup_id(&self) -> Option<i64> {
        match &self.correlation {
            Correlation::Supergroup(group) => Some(*group),
            _ => None,
        }
    }

    pub fn community_id(&self) -> Option<i64> {
        match &self.correlation {
            Correlation::Community(community) => Some(*community),
            _ => None,
        }
    }

    pub fn story_id(&self) -> Option<i32> {
        match &self.correlation {
            Correlation::Story(story) => Some(*story),
            _ => None,
        }
    }

    pub fn story_ids(&self) -> Option<&[i32]> {
        match &self.correlation {
            Correlation::StoryAlbum { story_ids, .. } => story_ids.as_deref(),
            _ => None,
        }
    }

    pub fn story_album_id(&self) -> Option<i32> {
        match &self.correlation {
            Correlation::StoryAlbum { album_id, .. } => *album_id,
            _ => None,
        }
    }

    pub fn user_ids(&self) -> Option<&[i64]> {
        match &self.correlation {
            Correlation::UserIds(ids) => Some(ids),
            _ => None,
        }
    }

    pub fn folder_id(&self) -> Option<i32> {
        match &self.correlation {
            Correlation::Folder(folder) => Some(*folder),
            _ => None,
        }
    }

    pub fn scope(&self) -> Option<NotificationSettingsScope> {
        match &self.correlation {
            Correlation::Scope(scope) => Some(*scope),
            _ => None,
        }
    }

    pub fn secret_chat_id(&self) -> Option<i32> {
        match &self.correlation {
            Correlation::SecretChat(id) => Some(*id),
            _ => None,
        }
    }

    pub fn invite_link(&self) -> Option<&str> {
        match &self.correlation {
            Correlation::InviteLink(link) => Some(link),
            _ => None,
        }
    }

    pub fn subject_id(&self) -> Option<&str> {
        match &self.correlation {
            Correlation::Subject(subject) => Some(subject),
            _ => None,
        }
    }

    pub fn file_id(&self) -> Option<i32> {
        match &self.correlation {
            Correlation::File(file) => Some(*file),
            _ => None,
        }
    }

    /// Whether the shared-media gallery's Stories tab asked for this page.
    pub fn is_gallery_tab_page(&self) -> bool {
        self.correlation == Correlation::GalleryTab
    }
}
