//! Request purposes for forum topics, comment threads and Saved Messages.
use crate::state::request_purpose::flat_purposes;

/// In-flight requests for forum topics, comment threads and Saved Messages; wrapped as
/// [`RequestPurpose::Threads`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadsPurpose {
    /// Phase 5.1: `getForumTopics` (first page). Response is `forumTopics`.
    GetForumTopics,
    /// Phase 5.1: `searchChatMessages` with `topic_id = messageTopicForum`
    /// and an empty query — per-topic history. Response is
    /// `foundChatMessages`; correlated via
    /// `PendingRequest::forum_topic_id`.
    GetTopicHistory,
    /// Slice G2: forum topic management (schema 1.8.67, lines
    /// 12665/12674/12713/12725/12736/12718). `createForumTopic`
    /// answers `forumTopicInfo`; the rest answer `ok`. The topic list is
    /// refetched on success; `updateForumTopicInfo` keeps it fresh
    /// otherwise.
    CreateForumTopic,
    EditForumTopic {
        forum_topic_id: i32,
    },
    ToggleForumTopicClosed {
        forum_topic_id: i32,
    },
    ToggleForumTopicPinned {
        forum_topic_id: i32,
    },
    DeleteForumTopic {
        forum_topic_id: i32,
    },
    ToggleGeneralForumTopicHidden,
    /// Subsection tabs: the tab menu's "Mark as read" — `viewMessages`
    /// on the topic's last message (`messageSourceForumTopicHistory`,
    /// schema 1.8.67 lines 13230 / 3213). Answers `ok`.
    ReadForumTopic {
        forum_topic_id: i32,
    },
    /// Subsection tabs: the tab menu's Mute / Unmute —
    /// `setForumTopicNotificationSettings` (schema 1.8.67, line 12707).
    SetForumTopicNotificationSettings {
        forum_topic_id: i32,
    },
    /// Subsection tabs: `getForumTopic` (schema 1.8.67, line 12679) — one
    /// topic's authoritative state (unread count, read position), answered
    /// with `forumTopic`.
    GetForumTopic {
        forum_topic_id: i32,
    },
    /// `toggleChatViewAsTopics` (schema 1.8.67, line 13513). Answers `ok`;
    /// `updateChatViewAsTopics` carries the new value.
    ToggleChatViewAsTopics,
    /// `getForumTopicDefaultIcons` (schema 1.8.67, line 12658): `stickers`.
    GetForumTopicDefaultIcons,
    /// `getForumTopicLink` (schema 1.8.67, line 12692): `messageLink`,
    /// copied to the clipboard like a message link.
    GetForumTopicLink,
    /// `setPinnedForumTopics` (schema 1.8.67, line 12730): `ok`.
    SetPinnedForumTopics,
    /// `readAllForumTopicMentions` (schema 1.8.67, line 12741): `ok`.
    ReadAllForumTopicMentions {
        forum_topic_id: i32,
    },
    /// `readAllForumTopicReactions` (schema 1.8.67, line 12746): `ok`.
    ReadAllForumTopicReactions {
        forum_topic_id: i32,
    },
    /// `unpinAllForumTopicMessages` (schema 1.8.67, line 12756): `ok`.
    UnpinAllForumTopicMessages {
        forum_topic_id: i32,
    },
    /// `loadSavedMessagesTopics` (schema 1.8.67, line 11765): `ok`; the
    /// sublists arrive as `updateSavedMessagesTopic`, a 404 means all
    /// of them were loaded.
    LoadSavedMessagesTopics,
    /// `deleteSavedMessagesTopicHistory` (schema 1.8.67, line 11781): `ok`.
    DeleteSavedMessagesTopicHistory {
        topic_id: i64,
    },
    /// `toggleSavedMessagesTopicIsPinned` (schema 1.8.67, line 11792): `ok`.
    ToggleSavedMessagesTopicPinned {
        topic_id: i64,
    },
    /// `setSavedMessagesTagLabel` (schema 1.8.67, line 12859): `ok`.
    SetSavedMessagesTagLabel,
    /// `getMessageThreadHistory` (schema 1.8.68, line 12231) with a
    /// negative offset: the page after the open thread's newest loaded
    /// reply, while the window stops short of the thread's last reply
    /// (`ThreadView::has_newer`). Response is `messages`; `message_id` is
    /// the thread's origin message, correlated to the origin chat via
    /// `PendingRequest::chat_id`.
    GetMessageThreadHistoryNewer {
        message_id: i64,
    },
}

flat_purposes!(Threads(ThreadsPurpose) {
    GetForumTopics,
    GetTopicHistory,
    CreateForumTopic,
    ToggleGeneralForumTopicHidden,
    ToggleChatViewAsTopics,
    GetForumTopicDefaultIcons,
    GetForumTopicLink,
    SetPinnedForumTopics,
    LoadSavedMessagesTopics,
    SetSavedMessagesTagLabel,
});
