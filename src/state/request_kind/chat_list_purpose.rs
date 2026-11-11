//! Generated-once classification of every `ChatListPurpose` variant.
use super::RequestKind;
use crate::state::ChatListPurpose;

impl ChatListPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::LoadChats
            | Self::LoadFolderChats
            | Self::LoadArchiveChats
            | Self::GetChatPreview
            | Self::GetArchiveChatListSettings
            | Self::GetChatFolder
            | Self::GetChatListsToAddChat
            | Self::GetChatFolderChatsToLeave
            | Self::GetRecommendedChatFolders
            | Self::GetChatsForFolderInviteLink
            | Self::GetChatFolderInviteLinks
            | Self::GetFolderInviteChat
            | Self::GetChatFolderNewChats
            | Self::GetPremiumLimit => RequestKind::Lookup,
            Self::AddChatToList
            | Self::DeleteChat
            | Self::ToggleChatIsPinned
            | Self::ToggleChatIsMarkedAsUnread
            | Self::DeleteChatHistory
            | Self::RemoveChatFromList
            | Self::HidePromotion
            | Self::LeaveBeforeDelete
            | Self::SetPinnedChats
            | Self::ReadChatList
            | Self::ReadChatInbox
            | Self::ClearRecentlyFoundChats
            | Self::SetArchiveChatListSettings
            | Self::CreateChatFolder
            | Self::EditChatFolder
            | Self::DeleteChatFolder
            | Self::ReorderChatFolders
            | Self::ToggleChatFolderTags
            | Self::CreateChatFolderInviteLink
            | Self::EditChatFolderInviteLink
            | Self::DeleteChatFolderInviteLink
            | Self::CheckChatFolderInviteLink
            | Self::AddChatFolderByInviteLink
            | Self::ProcessChatFolderNewChats => RequestKind::UserAction,
        }
    }
}
