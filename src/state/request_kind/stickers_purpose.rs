//! Generated-once classification of every `StickersPurpose` variant.
use super::RequestKind;
use crate::state::StickersPurpose;

impl StickersPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::Creator(inner) => inner.kind(),
            Self::ViewTrendingEmojiSets
            | Self::EmojiAnimations
            | Self::GetMessageEffect { .. }
            | Self::LoadLibrarySet { .. }
            | Self::ViewStickerSet { .. }
            | Self::CustomEmojiPack { .. }
            | Self::EmojiPackTitle { .. }
            | Self::GetInstalledStickerSets
            | Self::GetArchivedStickerSets
            | Self::GetStickerSet
            | Self::GetTrendingStickerSets
            | Self::ViewTrendingStickerSets
            | Self::SearchStickerSets
            | Self::SearchStickers
            | Self::GetFavoriteStickers
            | Self::GetRecentStickers
            | Self::GetInstalledMaskSets
            | Self::GetArchivedMaskSets { .. }
            | Self::GetMaskSet { .. }
            | Self::GetRecentEmojiStatuses
            | Self::GetThemedEmojiStatuses
            | Self::GetDefaultEmojiStatuses
            | Self::GetDefaultChatEmojiStatuses
            | Self::GetUpgradedGiftEmojiStatuses
            | Self::GetAnimatedEmoji
            | Self::GetCustomEmojiStickers
            | Self::SearchEmojis
            | Self::GetKeywordEmojis
            | Self::GetAttachedStickerSets { .. }
            | Self::GetGreetingStickers
            | Self::GetEmojiCategories
            | Self::GetStickerCategories
            | Self::GetInstalledEmojiSets
            | Self::GetArchivedEmojiSets { .. }
            | Self::GetTrendingEmojiSets
            | Self::SearchEmojiSets
            | Self::GetSavedAnimations
            | Self::ResolveGifSearchBot
            | Self::GetGifSearchResults { .. } => RequestKind::Lookup,
            Self::ClickAnimatedEmoji { .. }
            | Self::SuggestStickers
            | Self::AddFavoriteSticker
            | Self::RemoveFavoriteSticker
            | Self::ClearRecentStickers
            | Self::ChangeStickerSet
            | Self::ManageStickerSet { .. }
            | Self::ReorderInstalledStickerSets
            | Self::ChangeMaskSet { .. }
            | Self::ReorderInstalledMaskSets
            | Self::SetEmojiStatus
            | Self::ClearRecentEmojiStatuses
            | Self::SetDefaultReactionType
            | Self::RemoveRecentSticker
            | Self::ChangeEmojiSet
            | Self::ReorderInstalledEmojiSets
            | Self::AddSavedAnimation
            | Self::RemoveSavedAnimation => RequestKind::UserAction,
        }
    }
}
