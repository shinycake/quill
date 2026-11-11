//! Generated-once classification of every `ThreadsPurpose` variant.
use super::RequestKind;
use crate::state::ThreadsPurpose;

impl ThreadsPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::GetForumTopics
            | Self::GetTopicHistory
            | Self::GetTopicHistoryNewer
            | Self::GetForumTopic { .. }
            | Self::GetForumTopicDefaultIcons
            | Self::GetForumTopicLink
            | Self::LoadSavedMessagesTopics
            | Self::GetMessageThreadHistoryNewer { .. }
            | Self::LoadDirectMessagesChatTopics
            | Self::GetDirectMessagesChatTopic { .. }
            | Self::GetDirectMessagesChatTopicRevenue { .. } => RequestKind::Lookup,
            Self::CreateForumTopic
            | Self::EditForumTopic { .. }
            | Self::ToggleForumTopicClosed { .. }
            | Self::ToggleForumTopicPinned { .. }
            | Self::DeleteForumTopic { .. }
            | Self::ToggleGeneralForumTopicHidden
            | Self::ReadForumTopic { .. }
            | Self::SetForumTopicNotificationSettings { .. }
            | Self::ToggleChatViewAsTopics
            | Self::SetPinnedForumTopics
            | Self::ReadAllForumTopicMentions { .. }
            | Self::ReadAllForumTopicReactions { .. }
            | Self::UnpinAllForumTopicMessages { .. }
            | Self::DeleteSavedMessagesTopicHistory { .. }
            | Self::ToggleSavedMessagesTopicPinned { .. }
            | Self::SetSavedMessagesTagLabel
            | Self::DeleteDirectMessagesChatTopicHistory { .. }
            | Self::SetDirectMessagesChatTopicMarkedAsUnread { .. }
            | Self::ReadDirectMessagesChatTopic { .. }
            | Self::ToggleDirectMessagesChatTopicCanSendUnpaidMessages { .. } => {
                RequestKind::UserAction
            }
        }
    }
}
