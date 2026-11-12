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
                self.stickers.emoji.sticker_categories.failed(
                    pending.map(|p| p.id),
                    call_request_error_line(
                        err,
                        crate::lang::tr(
                            crate::lang::keys::Q_MEDIA_COULD_NOT_LOAD_STICKER_CATEGORIES,
                        ),
                    ),
                    false,
                );
            }
            Some(RequestPurpose::GetGreetingStickers) => {
                self.stickers.stickers.greeting.failed(
                    pending.map(|p| p.id),
                    call_request_error_line(
                        err,
                        crate::lang::tr(
                            crate::lang::keys::Q_MEDIA_COULDNT_UPDATE_STICKERS_TRY_AGAIN,
                        ),
                    ),
                    false,
                );
            }
            Some(RequestPurpose::SearchEmojis) => {
                if let Some(pending) = pending {
                    self.stickers.emoji.keyword_lookups.failed(pending.id);
                }
            }
            Some(RequestPurpose::Stickers(StickersPurpose::GetMessageEffect { effect_id })) => {
                if let Some(pending) = pending {
                    self.stickers.effects.fail(
                        effect_id,
                        pending.id,
                        err.code,
                        std::time::Instant::now(),
                    );
                }
            }
            Some(RequestPurpose::Stickers(StickersPurpose::EmojiPackTitle { set_id })) => {
                // The ask failed: the next menu asks for the title again.
                let titles = &mut self.stickers.emoji_pack_titles;
                if matches!(titles.get(&set_id), Some(PackTitle::Asking(asked)) if Some(*asked) == pending.map(|p| p.id))
                {
                    titles.remove(&set_id);
                }
            }
            Some(RequestPurpose::Stickers(StickersPurpose::ViewStickerSet { set_id })) => {
                self.fail_sticker_set_view(set_id)
            }
            Some(RequestPurpose::Stickers(StickersPurpose::CustomEmojiPack { .. })) => {
                if let Some(pending) = pending {
                    self.fail_custom_emoji_preview(pending.id);
                }
            }
            // "Attached Stickers": the box waiting for this request says it
            // failed.
            Some(RequestPurpose::Stickers(StickersPurpose::GetAttachedStickerSets { .. })) => {
                if self
                    .stickers
                    .stickers
                    .attached
                    .waits_for(pending.map(|p| p.id))
                {
                    self.stickers.stickers.attached = AttachedSets::Idle;
                    self.fail_sticker_set_view(0);
                }
            }
            // The lookup's ids are asked for again. A rate limit is no
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
            }
            _ => {}
        }
        // Slice G2: `boostChat` failed — the status is refetched on
        // success only, so nothing to roll back.
        if let Some(pending) = pending.filter(|p| p.purpose == RequestPurpose::GetStickerSet) {
            // Only the request the panel waits for fails the set view.
            let panel = &mut self.stickers.stickers;
            if let StickerSetStage::Loading { set_id, request } = panel.selection.stage
                && request == pending.id
            {
                panel.selection.stage = StickerSetStage::Failed { set_id };
            }
        }
        if let Some(RequestPurpose::Stickers(StickersPurpose::LoadLibrarySet { set_id })) =
            pending.map(|p| p.purpose)
        {
            self.fail_library_set(set_id);
        }
        if let Some(pending) = pending
            && matches!(
                pending.purpose,
                RequestPurpose::Stickers(StickersPurpose::ManageStickerSet { .. })
            )
        {
            self.finish_sticker_batch_item(pending.id, false);
        }
        if let Some(pending) = pending
            && let RequestPurpose::Stickers(StickersPurpose::ManageStickerSet { set_id, .. }) =
                pending.purpose
        {
            self.finish_set_change(pending.id, set_id, false);
        }
        self.fail_mask_request(pending);
        self.fail_sticker_creator(err, pending);
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ChangeEmojiSet) {
            // Only the change the box waits for fails it.
            if self
                .stickers
                .emoji
                .mutation
                .waits_for(pending.map(|p| p.id))
            {
                self.stickers.emoji.mutation = crate::emoji::SetMutation::Failed;
            }
        }
        // Suggestions that waited for the installed sets stop waiting: the
        // next emoji typed asks again.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledStickerSets)
            && self.stickers.stickers.suggest_for.waiting_for_sets()
        {
            self.stickers.stickers.suggest_for = SuggestFor::Nothing;
        }
        // The composer's suggestions asked for an emoji and failed: the next
        // keystroke asks again.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::SuggestStickers)
            && self
                .stickers
                .stickers
                .suggest_for
                .waits_for(pending.map(|p| p.id))
        {
            self.stickers.stickers.suggest_for = SuggestFor::Nothing;
        }
        let emoji_purpose = pending.map(|p| p.purpose);
        self.stickers
            .emoji
            .status_change
            .failed(pending.map(|p| p.id));
        if emoji_purpose == Some(RequestPurpose::GetTrendingEmojiSets) {
            self.stickers.emoji.lists.trending.failed_request(
                pending.map(|p| p.id),
                crate::lang::tr(crate::lang::keys::Q_MEDIA_COULD_NOT_UPDATE_EMOJI_PACKS),
            );
        }
        if emoji_purpose == Some(RequestPurpose::GetKeywordEmojis) {
            self.stickers
                .emoji
                .keyword_emojis
                .failed_request(pending.map(|p| p.id));
        }
        if emoji_purpose == Some(RequestPurpose::GetAnimatedEmoji) {
            self.stickers
                .emoji
                .animated
                .failed_request(pending.map(|p| p.id));
        }
        if let Some(list) = emoji_purpose.and_then(|p| self.stickers.emoji.lists.for_purpose(p)) {
            list.failed(
                pending.map(|p| p.id),
                crate::lang::tr(crate::lang::keys::Q_MEDIA_COULD_NOT_UPDATE_EMOJI_PACKS),
                false,
            );
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
            // A list that failed to load says so on its tab; a step of the
            // manager box's saved changes is reported by the manager (the
            // box is closed by then); another change that failed leaves the
            // lists as they are and reports a note.
            if let Some(pending) = pending {
                let line =
                    crate::lang::tr(crate::lang::keys::Q_MEDIA_COULDNT_UPDATE_STICKERS_TRY_AGAIN)
                        .to_string();
                match self.stickers.stickers.lists.for_purpose(pending.purpose) {
                    Some(list) => {
                        list.failed(Some(pending.id), line, false);
                    }
                    None if self.stickers.manager.note_commit_failure(pending.id) => {}
                    None => self.stickers.creator_notes.push_back(line),
                }
            }
        }
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::ResolveGifSearchBot) => self.stickers.gifs.search_failed_now(),
            Some(RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { .. })) => {
                self.stickers
                    .gifs
                    .search_request_failed(pending.map(|p| p.id));
            }
            _ => {}
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::AddSavedAnimation | RequestPurpose::RemoveSavedAnimation)
        ) {
            self.stickers.gifs.saved_mutation_failed();
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedAnimations) {
            self.stickers.gifs.saved_failed();
        }
    }
}
