//! Generated-once classification of every `SearchPurpose` variant.
use super::RequestKind;
use crate::state::SearchPurpose;

impl SearchPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::SearchChats
            | Self::SearchMessages
            | Self::SearchRecentlyFoundChats
            | Self::SearchPublicChats
            | Self::SearchChatsOnServer
            | Self::SearchPublicPosts
            | Self::SearchPublicMessagesByTag
            | Self::SearchMessagesMore
            | Self::SearchPublicPostsMore
            | Self::GetRecommendedChats
            | Self::GetPublicPostSearchLimits
            | Self::GetTopChats
            | Self::SearchChatMessages
            | Self::SearchChatMessagesMore
            | Self::SearchFromMembers
            | Self::GetChatMessageByDate
            | Self::GetChatMessageCalendar { .. }
            | Self::ViewSponsoredChat => RequestKind::Lookup,
            Self::AddRecentlyFoundChat
            | Self::RemoveTopChat
            | Self::RemoveRecentlyFoundChat
            | Self::SetTopChatsDisabled => RequestKind::UserAction,
        }
    }
}
