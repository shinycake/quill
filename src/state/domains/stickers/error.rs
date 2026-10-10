//! Failed requests for stickers, custom emoji, emoji statuses, reactions, GIFs and the media library.
use crate::state::*;

impl Session {
    /// Reacts to a failed stickers request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_stickers_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Stickers(StickersPurpose::ViewStickerSet { set_id })) => {
                self.fail_sticker_set_view(set_id)
            }
            Some(
                RequestPurpose::SetEmojiStatus
                | RequestPurpose::ClearRecentEmojiStatuses
                | RequestPurpose::GetRecentEmojiStatuses
                | RequestPurpose::GetThemedEmojiStatuses
                | RequestPurpose::GetDefaultEmojiStatuses
                | RequestPurpose::GetCustomEmojiStickers,
            ) => {
                self.emoji.status_note = Some(call_request_error_line(
                    err,
                    "Could not update emoji statuses. Retry the action",
                ));
            }
            _ => {}
        }
        // Slice G2: `boostChat` failed — the status is refetched on
        // success only, so nothing to roll back.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledStickerSets) {
            self.stickers.loading_sets = false;
            self.stickers.failed = true;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStickerSet) {
            self.stickers.loading_set = false;
            self.stickers.failed = true;
        }
        if let Some(RequestPurpose::Stickers(StickersPurpose::LoadLibrarySet { set_id })) =
            pending.map(|p| p.purpose)
        {
            self.fail_library_set(set_id);
        }
        if let Some(RequestPurpose::Stickers(StickersPurpose::ManageStickerSet {
            set_id, ..
        })) = pending.map(|p| p.purpose)
        {
            self.finish_sticker_batch_item(set_id, false);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ChangeEmojiSet) {
            self.emoji.mutation_failed = true;
            self.emoji.mutating_set = None;
        }
        let emoji_purpose = pending.map(|p| p.purpose);
        if emoji_purpose == Some(RequestPurpose::SetEmojiStatus) {
            self.emoji.pending_status_emoji = None;
        }
        if emoji_purpose == Some(RequestPurpose::GetEmojiSet)
            || emoji_purpose == Some(RequestPurpose::ChangeEmojiSet)
            || (emoji_purpose == Some(RequestPurpose::GetInstalledEmojiSets)
                && self.emoji.tab == crate::emoji::EmojiSetTab::Installed)
            || (emoji_purpose == Some(RequestPurpose::GetTrendingEmojiSets)
                && self.emoji.tab == crate::emoji::EmojiSetTab::Trending)
            || (emoji_purpose == Some(RequestPurpose::SearchEmojiSets)
                && self.emoji.tab == crate::emoji::EmojiSetTab::Search)
        {
            self.emoji.failed = true;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::Stickers(StickersPurpose::ManageStickerSet { .. })
                    | RequestPurpose::ReorderInstalledStickerSets
                    | RequestPurpose::SearchStickers
                    | RequestPurpose::SearchStickerSets
                    | RequestPurpose::GetFavoriteStickers
                    | RequestPurpose::GetRecentStickers
                    | RequestPurpose::GetArchivedStickerSets
                    | RequestPurpose::GetTrendingStickerSets
                    | RequestPurpose::ClearRecentStickers
                    | RequestPurpose::AddFavoriteSticker
                    | RequestPurpose::RemoveFavoriteSticker
            )
        ) {
            self.stickers.failed = true;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::ResolveGifSearchBot
                    | RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { .. })
            )
        ) {
            self.gifs.search_failed = true;
            self.gifs.search_loading = false;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::AddSavedAnimation | RequestPurpose::RemoveSavedAnimation)
        ) {
            self.gifs.failed = true;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedAnimations) {
            self.gifs.loading = false;
            self.gifs.loaded = true;
            self.gifs.failed = true;
            self.gifs.stale = false;
        }
    }
}
