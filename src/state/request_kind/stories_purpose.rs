//! Generated-once classification of every `StoriesPurpose` variant.
use super::RequestKind;
use crate::state::StoriesPurpose;

impl StoriesPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::Live(inner) => inner.kind(),
            Self::LoadActiveStories
            | Self::LoadHiddenStories
            | Self::GetChatActiveStories
            | Self::GetStory
            | Self::OpenStory
            | Self::CloseStory
            | Self::GetStoryAvailableReactions
            | Self::GetStoryCustomEmojiStickers
            | Self::GetStoryInteractions
            | Self::GetStoryStatistics
            | Self::GetStoryPublicForwards
            | Self::SearchPublicStories
            | Self::GetChatsToPostStories
            | Self::GetChatStoryAlbums
            | Self::GetStoryAlbumStories
            | Self::GetChatArchivedStories
            | Self::GetChatPostedToChatPageStories
            | Self::GetCloseFriends => RequestKind::Lookup,
            Self::SetStoryReaction
            | Self::RemoveStoryReaction
            | Self::DeleteStory
            | Self::SendStoryReply
            | Self::ReportStory
            | Self::ActivateStoryStealthMode
            | Self::CheckCanPostStory
            | Self::PostStory
            | Self::EditStory
            | Self::EditStoryCover
            | Self::SetStoryPrivacySettings
            | Self::CreateStoryAlbum
            | Self::ReorderStoryAlbums
            | Self::DeleteStoryAlbum
            | Self::SetStoryAlbumName
            | Self::AddStoryAlbumStories
            | Self::RemoveStoryAlbumStories
            | Self::ReorderStoryAlbumStories
            | Self::SetChatPinnedStories
            | Self::SetCloseFriends
            | Self::SetChatActiveStoriesList
            | Self::ToggleStoryIsPostedToChatPage => RequestKind::UserAction,
        }
    }
}
