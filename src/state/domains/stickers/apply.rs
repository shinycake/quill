//! Applies TDLib updates and answers for stickers, custom emoji, emoji statuses, reactions, GIFs and the media library.
use crate::state::*;
use crate::telegram::envelope::StickersPayload;

impl Session {
    /// Applies one stickers payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_stickers_payload(
        &mut self,
        payload: StickersPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            StickersPayload::UpdateActiveEmojiReactions { emojis } => {
                self.active_reactions = emojis.clone();
                self.active_emoji_reactions = emojis;
                self.reaction_options_stale = true;
            }
            StickersPayload::StickerSets { sets, .. } => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::Stickers(
                        StickersPurpose::GetAttachedStickerSets { .. }
                    ))
                ) {
                    // B11: "Attached Stickers" of a photo or video.
                    self.stickers.attached_answer = Some(sets.first().map(|set| set.id));
                } else if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::GetInstalledStickerSets)
                {
                    self.accept_installed_sticker_sets(sets);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetArchivedStickerSets)
                {
                    self.accept_archived_sticker_sets(sets);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::SearchStickerSets) {
                    // Slice S8: `searchStickerSets` answers with `stickerSets`.
                    self.accept_found_sticker_sets(sets);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledEmojiSets)
                {
                    // Slice S10: emoji `stickerSets` land in the emoji panel (see emoji.rs).
                    self.accept_installed_emoji_sets(sets);
                } else if let Some(RequestPurpose::Stickers(
                    StickersPurpose::GetArchivedEmojiSets { first_page },
                )) = pending.map(|p| p.purpose)
                {
                    self.accept_archived_emoji_sets(sets, first_page);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::SearchEmojiSets) {
                    self.accept_found_emoji_sets(sets);
                }
            }
            // Slice S15: `updateInstalledStickerSets` — TDLib's authoritative
            // new order after a usage-driven (sticker send with
            // `update_order_of_installed_sticker_sets`) or manual reorder.
            // Applied in place so an open panel reshuffles live; an empty
            // cache is a no-op and the next fetch arrives ordered.
            StickersPayload::UpdateInstalledStickerSets {
                sticker_set_ids,
                is_regular,
            } => {
                self.apply_installed_sticker_set_order(&sticker_set_ids, is_regular);
            }
            // B11: other devices changed recents, favorites, trending or the
            // reaction lists; mark the loaded caches stale (the driver
            // refetches) or store the pushed value.
            StickersPayload::UpdateRecentStickers { is_attached } => {
                if !is_attached {
                    self.stickers.recent_stale = true;
                }
            }
            StickersPayload::UpdateFavoriteStickers => self.stickers.favorites_stale = true,
            StickersPayload::UpdateTrendingStickerSets { is_regular } => {
                if is_regular {
                    self.stickers.trending_stale = true;
                } else {
                    self.emoji.trending_stale = true;
                }
            }
            StickersPayload::UpdateDefaultReactionType { reaction_type } => {
                self.default_reaction = ReactionChoice::from_type(&reaction_type);
            }
            // Slice S8: `getTrendingStickerSets` answers with
            // `trendingStickerSets`.
            StickersPayload::TrendingStickerSets {
                sets,
                is_premium,
                total_count,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetTrendingStickerSets) {
                    self.stickers.trending_total = total_count.max(0) as usize;
                    self.accept_trending_sticker_sets(sets, is_premium);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetTrendingEmojiSets) {
                    // Slice S10: emoji `trendingStickerSets` land in the emoji panel (see emoji.rs).
                    self.emoji.trending_total = total_count;
                    self.accept_trending_emoji_sets(sets, is_premium);
                }
            }
            // Slice S8: `searchStickers` / `getFavoriteStickers` /
            // `getRecentStickers` answer with bare `stickers`.
            StickersPayload::Stickers { stickers, files } => {
                self.remember_files(&files);
                let purpose = pending.map(|p| p.purpose);
                if purpose == Some(RequestPurpose::SearchStickers) {
                    self.accept_found_stickers(stickers);
                } else if purpose == Some(RequestPurpose::SuggestStickers) {
                    // Slice S12: composer suggestions land in their own
                    // slot — never the search UI's `found_stickers`.
                    self.accept_sticker_suggestions(stickers);
                } else if purpose == Some(RequestPurpose::GetFavoriteStickers) {
                    self.accept_favorite_stickers(stickers);
                } else if purpose == Some(RequestPurpose::GetRecentStickers) {
                    self.accept_recent_stickers(stickers);
                } else if purpose == Some(RequestPurpose::GetGreetingStickers) {
                    self.stickers.greeting = stickers;
                    self.stickers.greeting_loaded = true;
                } else if purpose == Some(RequestPurpose::GetCustomEmojiStickers) {
                    // Slice S10: bare `stickers` land in the emoji panel (see emoji.rs).
                    self.accept_custom_emoji_stickers(stickers);
                } else if purpose == Some(RequestPurpose::GetForumTopicDefaultIcons) {
                    self.accept_topic_default_icons(stickers);
                } else if purpose == Some(RequestPurpose::GetStoryCustomEmojiStickers) {
                    // Phase 9.2+: story reaction picker visuals — keyed by
                    // sticker id (= custom emoji id).
                    self.accept_story_custom_emoji_stickers(stickers);
                }
            }
            // Slice S10: emoji payloads — purpose-gated dispatch lives in emoji.rs.
            payload @ (StickersPayload::EmojiStatuses { .. }
            | StickersPayload::EmojiStatusCustomEmojis { .. }
            | StickersPayload::AnimatedEmoji { .. }
            | StickersPayload::EmojiKeywords { .. }
            | StickersPayload::Emojis { .. }
            | StickersPayload::EmojiCategories { .. }) => {
                self.dispatch_emoji_payload(pending.map(|p| p.purpose), payload.into());
            }
            StickersPayload::UpdateStickerSet {
                id,
                is_custom_emoji,
            } => {
                if is_custom_emoji && id > 0 {
                    self.emoji.outdated_packs.insert(id);
                    if self.emoji.selected_set_id == Some(id) {
                        drop(self.requests.take_purpose(RequestPurpose::GetEmojiSet));
                    }
                }
            }
            StickersPayload::StickerSet {
                id,
                stickers,
                files,
                title,
                name,
                is_installed,
                is_custom_emoji,
                ..
            } => {
                if let Some(RequestPurpose::Chats(ChatsPurpose::DeepLinkResolve { generation })) =
                    pending.map(|p| p.purpose)
                    && matches!(
                        &self.chats_state.deep_link,
                        Some(DeepLinkState::ResolvingChat {
                            action: DeepLinkAction::StickerSet { .. },
                            generation: slot,
                        }) if *slot == generation
                    )
                {
                    // `addstickers` / `addemoji` link: hand the set id to the
                    // preview dialog (`getStickerSet` loads its stickers).
                    self.chats_state.deep_link = Some(DeepLinkState::Ui(
                        crate::deep_link_types::DeepLinkUi::StickerSet { set_id: id },
                    ));
                } else if let Some(RequestPurpose::Stickers(StickersPurpose::ViewStickerSet {
                    set_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.remember_files(&files);
                    self.accept_sticker_set_view(
                        set_id,
                        title,
                        name,
                        is_installed,
                        is_custom_emoji,
                        stickers,
                    );
                } else if let Some(RequestPurpose::Stickers(StickersPurpose::CustomEmojiPack {
                    emoji_id,
                    set_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.accept_custom_emoji_preview(emoji_id, set_id, title);
                } else if let Some(RequestPurpose::Stickers(StickersPurpose::EmojiPackTitle {
                    set_id,
                })) = pending.map(|p| p.purpose)
                {
                    if !title.is_empty() {
                        self.emoji_pack_titles.insert(set_id, title);
                    }
                } else if let Some(RequestPurpose::Stickers(StickersPurpose::LoadLibrarySet {
                    set_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.remember_files(&files);
                    self.accept_library_set(set_id, stickers);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetEmojiSet)
                    && self.emoji.selected_set_id == Some(id)
                {
                    self.remember_files(&files);
                    self.emoji.failed = false;
                    self.emoji.preview_title = title;
                    self.emoji
                        .pack_files
                        .insert(id, stickers.iter().map(|item| item.file_id).collect());
                    self.emoji.outdated_packs.remove(&id);
                    self.emoji.preview = stickers;
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStickerSet) {
                    self.remember_files(&files);
                    self.accept_sticker_set(id, stickers);
                }
            }
            StickersPayload::Animations { animations, files } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedAnimations) {
                    self.remember_files(&files);
                    self.accept_saved_animations(animations);
                }
            }
            StickersPayload::UpdateSavedAnimations { .. } => {
                self.gifs.stale = true;
            }
            // Slice S9: `updateAnimationSearchParameters` (schema 1.8.67,
            // line 11064) — server-pushed; store the provider name and the
            // new suggested search emojis for the GIF search surface.
            StickersPayload::UpdateAnimationSearchParameters { provider, emojis } => {
                self.gifs.search_provider = provider;
                self.gifs.provider_emojis = emojis;
            }
        }
    }
}
