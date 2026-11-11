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
            Some(RequestPurpose::EmojiAnimations) => self.stickers.animations.failed(),
            Some(RequestPurpose::GetStickerCategories) => {
                self.stickers
                    .emoji
                    .sticker_categories
                    .fail(call_request_error_line(
                        err,
                        crate::lang::tr(
                            crate::lang::keys::Q_MEDIA_COULD_NOT_LOAD_STICKER_CATEGORIES,
                        ),
                    ));
            }
            Some(RequestPurpose::SearchEmojis) => {
                if let Some(pending) = pending {
                    self.stickers.emoji.keyword_lookups.failed(pending.id);
                }
            }
            Some(RequestPurpose::Stickers(StickersPurpose::GetMessageEffect { effect_id })) => self
                .stickers
                .effects
                .fail(effect_id, err.code, std::time::Instant::now()),
            Some(RequestPurpose::Stickers(StickersPurpose::ViewStickerSet { set_id })) => {
                self.fail_sticker_set_view(set_id)
            }
            // "Attached Stickers": the box it opened (set 0) says it failed.
            Some(RequestPurpose::Stickers(StickersPurpose::GetAttachedStickerSets { .. })) => {
                if pending.map(|p| p.id) == self.stickers.stickers.attached_request {
                    self.stickers.stickers.attached_request = None;
                    self.fail_sticker_set_view(0);
                }
            }
            // The lookup's ids are asked for again; the note shows only
            // where it is read, in the status picker. A rate limit is no
            // failure: the connect layer holds lookups until it is over.
            Some(RequestPurpose::GetCustomEmojiStickers) => {
                let emoji = &mut self.stickers.emoji;
                if let Some(pending) = pending {
                    if err.is_flood() {
                        emoji.custom_emoji_lookup_rate_limited(pending.id);
                    } else {
                        emoji.custom_emoji_lookup_failed(pending.id);
                    }
                }
                if emoji.status_open || emoji.chat_status_open {
                    emoji.status_note = Some(call_request_error_line(
                        err,
                        "Could not update emoji statuses. Retry the action",
                    ));
                }
            }
            Some(
                RequestPurpose::SetEmojiStatus
                | RequestPurpose::ClearRecentEmojiStatuses
                | RequestPurpose::GetRecentEmojiStatuses
                | RequestPurpose::GetThemedEmojiStatuses
                | RequestPurpose::GetDefaultEmojiStatuses
                | RequestPurpose::GetDefaultChatEmojiStatuses,
            ) => {
                self.stickers.emoji.status_note = Some(call_request_error_line(
                    err,
                    "Could not update emoji statuses. Retry the action",
                ));
            }
            _ => {}
        }
        // Slice G2: `boostChat` failed — the status is refetched on
        // success only, so nothing to roll back.
        if let Some(pending) = pending.filter(|p| p.purpose == RequestPurpose::GetStickerSet) {
            // Only the request the panel waits for fails the set view.
            let panel = &mut self.stickers.stickers;
            if let StickerSetStage::Loading { set_id, request } = panel.set
                && request == pending.id
            {
                panel.set = StickerSetStage::Failed { set_id };
            }
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
        self.fail_mask_request(pending.map(|p| p.purpose));
        self.fail_sticker_creator(err, pending);
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ChangeEmojiSet) {
            self.stickers.emoji.mutation_failed = true;
            self.stickers.emoji.mutating_set = None;
        }
        let emoji_purpose = pending.map(|p| p.purpose);
        if emoji_purpose == Some(RequestPurpose::SetEmojiStatus) {
            self.stickers.emoji.pending_status_emoji = None;
        }
        if emoji_purpose == Some(RequestPurpose::ChangeEmojiSet)
            || (emoji_purpose == Some(RequestPurpose::GetInstalledEmojiSets)
                && self.stickers.emoji.tab == crate::emoji::EmojiSetTab::Installed)
            || (emoji_purpose == Some(RequestPurpose::GetTrendingEmojiSets)
                && self.stickers.emoji.tab == crate::emoji::EmojiSetTab::Trending)
            || (emoji_purpose == Some(RequestPurpose::SearchEmojiSets)
                && self.stickers.emoji.tab == crate::emoji::EmojiSetTab::Search)
        {
            self.stickers.emoji.failed = true;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::GetInstalledStickerSets
                    | RequestPurpose::Stickers(StickersPurpose::ManageStickerSet { .. })
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
            // A list that failed to load says so on its tab; a change that
            // failed leaves the lists as they are and reports a note.
            if let Some(pending) = pending {
                let line =
                    crate::lang::tr(crate::lang::keys::Q_MEDIA_COULDNT_UPDATE_STICKERS_TRY_AGAIN)
                        .to_string();
                match self.stickers.stickers.lists.for_purpose(pending.purpose) {
                    Some(list) => list.fail(line),
                    None => self.stickers.creator_note = Some(line),
                }
            }
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::ResolveGifSearchBot
                    | RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { .. })
            )
        ) {
            self.stickers.gifs.search_failed = true;
            self.stickers.gifs.search_loading = false;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::AddSavedAnimation | RequestPurpose::RemoveSavedAnimation)
        ) {
            self.stickers.gifs.failed = true;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedAnimations) {
            self.stickers.gifs.loading = false;
            self.stickers.gifs.loaded = true;
            self.stickers.gifs.failed = true;
            self.stickers.gifs.stale = false;
        }
    }
}
